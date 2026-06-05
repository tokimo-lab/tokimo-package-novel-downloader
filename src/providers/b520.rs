use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct B520Provider;

#[async_trait]
impl Provider for B520Provider {
    fn name(&self) -> &str {
        "b520"
    }

    fn display_name(&self) -> &str {
        "笔趣阁(b520)"
    }

    fn base_url(&self) -> &str {
        "http://www.b520.cc"
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
            "http://www.b520.cc/modules/article/search.php?searchkey={}",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        // Search results are in a table with class "grid", rows after header
        if let Ok(sel) = Selector::parse("table.grid tr") {
            for (idx, elem) in doc.select(&sel).enumerate() {
                if idx == 0 {
                    continue; // skip header row
                }
                if results.len() >= limit {
                    break;
                }

                let href = select_attr_in(&elem, "td:first-child a", "href");
                if href.is_empty() {
                    continue;
                }

                let book_id = href
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let title = select_text_in(&elem, "td:first-child a");
                let latest_chapter = select_text_in(&elem, "td:nth-child(2) a");
                let author = select_text_in(&elem, "td:nth-child(3)");
                let word_count = select_text_in(&elem, "td:nth-child(4)");
                let update_date = select_text_in(&elem, "td:nth-child(5)");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date,
                    word_count,
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("http://www.b520.cc/{}/", book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "#info h1");

        let author_raw = select_text(&doc, "#info p:first-of-type");
        info.author = author_raw
            .replace('\u{00a0}', "")
            .replace("作者：", "")
            .trim()
            .to_string();

        info.cover_url = select_attr(&doc, "#fmimg img", "src");
        let update_raw = select_text(&doc, "#info p:nth-of-type(3)");
        info.update_time = update_raw.replace("最后更新：", "").trim().to_string();
        info.summary = select_text(&doc, "#intro p");

        // Parse chapters - after the "正文" dt element
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list dl dd a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .rsplit('/')
                        .next()
                        .unwrap_or(href)
                        .split('.')
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
        let url = format!("http://www.b520.cc/{}/{}.html", book_id, chapter_id);
        let html_str = client.get_with_encoding(&url, encoding_rs::GBK).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "div.bookname h1");
        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content p") {
            let paragraphs: Vec<String> = doc
                .select(&sel)
                .map(|p| element_text(&p))
                .filter(|t| !t.is_empty())
                .collect();
            content = paragraphs.join("\n");
        }

        if content.is_empty() {
            if let Ok(sel) = Selector::parse("#content") {
                if let Some(elem) = doc.select(&sel).next() {
                    content = html_to_text(&elem.inner_html());
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(B520Provider)
}
