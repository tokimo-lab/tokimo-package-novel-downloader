use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct TtkanProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(TtkanProvider)
}

/// Check if a line matches the obfuscated "www.ttkan.co" ad pattern.
fn is_ttkan_ad(line: &str) -> bool {
    const AD_SETS: &[&[char]] = &[
        &['W', 'w', 'ω', 'ш', 'щ'],
        &['W', 'w', 'ω', 'ш', 'щ'],
        &['W', 'w', 'ω', 'ш', 'щ'],
        &[
            '.', '¤', '¸', '•', '⊕', '⊙', '▪', '▲', '◆', '●', '★', '☢', '✿', '＿',
        ],
        &['T', 't', 'т', 'ⓣ'],
        &['T', 't', 'т', 'ⓣ'],
        &['K', 'k', 'κ', 'К', 'к', 'ⓚ'],
        &['a', 'á', 'ā', 'ǎ', 'Λ', 'д', 'ⓐ'],
        &['N', 'n', 'ⓝ'],
        &[
            '.', '¤', '¸', '•', '⊕', '⊙', '▪', '▲', '◆', '●', '★', '☢', '✿', '＿',
        ],
        &['C', 'c', 'С', '℃', '￠'],
        &['O', 'o', 'Ο', '○', '〇'],
    ];

    let cleaned: String = line.chars().filter(|c| *c != ' ').collect();
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() != AD_SETS.len() {
        return false;
    }
    let mut mismatches = 0;
    for (i, ch) in chars.iter().enumerate() {
        if !AD_SETS[i].contains(ch) {
            mismatches += 1;
            if mismatches > 2 {
                return false;
            }
        }
    }
    true
}

#[async_trait]
impl Provider for TtkanProvider {
    fn name(&self) -> &str {
        "ttkan"
    }

    fn display_name(&self) -> &str {
        "天天看小说"
    }

    fn base_url(&self) -> &str {
        "https://www.ttkan.co"
    }

    fn supports_search(&self) -> bool {
        true
    }

    async fn search(
        &self,
        client: &HttpClient,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://www.ttkan.co/novel/search?q={}",
            urlencoding::encode(keyword)
        );
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let selectors = ["div.novel_cell", "div.frame_body div.novel_cell"];
        for sel_str in &selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                let items: Vec<_> = doc.select(&sel).collect();
                if items.is_empty() {
                    continue;
                }
                for elem in items.into_iter().take(limit) {
                    let href = select_attr_in(&elem, "a[href]", "href");
                    if href.is_empty() {
                        continue;
                    }
                    let book_id = href
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let title = select_text_in(&elem, "h3");
                    let author = select_text_in(&elem, "li").replace("作者：", "");

                    results.push(SearchResult {
                        site: self.name().to_string(),
                        book_id,
                        title,
                        author,
                        latest_chapter: String::new(),
                        update_date: String::new(),
                        word_count: String::new(),
                    });
                }
                if !results.is_empty() {
                    break;
                }
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://cn.ttkan.co/novel/chapters/{}", book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.novel_info h1");
        info.author = {
            // Try to find <li> containing "作者：" and get the <a> text
            if let Ok(sel) = Selector::parse("div.novel_info li") {
                let mut found = String::new();
                for li in doc.select(&sel) {
                    let text = element_text(&li);
                    if text.contains("作者：") {
                        if let Ok(a_sel) = Selector::parse("a") {
                            if let Some(a) = li.select(&a_sel).next() {
                                found = element_text(&a);
                            }
                        }
                        break;
                    }
                }
                found
            } else {
                String::new()
            }
        };
        info.cover_url = select_attr(&doc, "div.novel_info amp-img", "src");
        info.serial_status = select_text(&doc, "div.novel_info span.state_serial");
        info.summary = select_text(&doc, "div.description p");

        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.full_chapters > div:first-child a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    // '/novel/pagea/wushenzhuzai-anmoshi_6094.html' -> '6094'
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('_')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url(), href),
                    });
                }
            }
        }

        if !chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: "正文".to_string(),
                chapters,
            });
        }

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "https://cn.wa01.com/novel/pagea/{}_{}.html",
            book_id, chapter_id
        );
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "div.title h1");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.content p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if !text.is_empty() && !is_ttkan_ad(&text) {
                    paragraphs.push(text);
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: paragraphs.join("\n"),
        })
    }
}
