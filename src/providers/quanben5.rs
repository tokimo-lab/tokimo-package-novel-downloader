use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct Quanben5Provider;

impl Quanben5Provider {
    const BASE_URL: &'static str = "https://quanben5.com";
    const SEARCH_URL: &'static str = "https://quanben5.com/";
    const STATIC_CHARS: &'static str =
        "PXhw7UT1B0a9kQDKZsjIASmOezxYG4CHo5Jyfg2b8FLpEvRr3WtVnlqMidu6cN";

    fn custom_base64(s: &str) -> String {
        let chars: Vec<char> = Self::STATIC_CHARS.chars().collect();
        let mut out = String::new();
        for ch in s.chars() {
            let idx = chars.iter().position(|&c| c == ch);
            let code = if let Some(i) = idx {
                chars[(i + 3) % 62]
            } else {
                ch
            };
            let n1 = rand_small() % 62;
            let n2 = rand_small() % 62;
            out.push(chars[n1]);
            out.push(code);
            out.push(chars[n2]);
        }
        out
    }
}

fn rand_small() -> usize {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut h = s.build_hasher();
    h.write_u8(0);
    (h.finish() as usize) % 62
}

#[async_trait]
impl Provider for Quanben5Provider {
    fn name(&self) -> &str {
        "quanben5"
    }

    fn base_url(&self) -> &str {
        Self::BASE_URL
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
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .to_string();
        let uri_keyword = urlencoding::encode(keyword).to_string();
        let b_raw = Quanben5Provider::custom_base64(&uri_keyword);
        let b = urlencoding::encode(&b_raw).to_string();

        let url = format!(
            "{}?c=book&a=search.json&callback=search&t={}&keywords={}&b={}",
            Self::SEARCH_URL, t, uri_keyword, b
        );

        let resp = client.get(&url).await?;

        // Unwrap JSONP: search({...});
        let json_str = if resp.starts_with("search(") && resp.ends_with(");") {
            &resp[7..resp.len() - 2]
        } else {
            &resp
        };

        let data: serde_json::Value = serde_json::from_str(json_str)?;
        let content_html = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if content_html.is_empty() {
            return Ok(vec![]);
        }

        let doc = Html::parse_fragment(content_html);
        let mut results = Vec::new();

        let sel = Selector::parse("div.pic_txt_list").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let href = select_attr_in(&elem, "h3 a", "href");
            if href.is_empty() {
                continue;
            }

            let book_id = href.trim_end_matches('/').rsplit('/').next()
                .unwrap_or("").to_string();

            let title = select_text_in(&elem, "h3 a span.name");
            let author = select_text_in(&elem, "p.info span.author");

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

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/n/{}/xiaoshuo.html", Self::BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "h3 span");
        info.author = select_text(&doc, "p.info span.author");
        info.cover_url = select_attr(&doc, "div.pic img", "src");
        info.summary = select_text(&doc, "p.description");

        // Parse chapters
        let mut chapters = Vec::new();
        let li_sel = Selector::parse("ul.list li").unwrap();
        let a_sel = Selector::parse("a").unwrap();
        let span_sel = Selector::parse("span").unwrap();

        for elem in doc.select(&li_sel) {
            if let Some(a) = elem.select(&a_sel).next() {
                let href = a.value().attr("href").unwrap_or("");
                let title = a.select(&span_sel).next()
                    .map(|s| element_text(&s))
                    .unwrap_or_default();
                if title.is_empty() || href.is_empty() {
                    continue;
                }
                let chapter_id = href.trim_end_matches(".html")
                    .rsplit('/').next().unwrap_or("").to_string();
                chapters.push(ChapterInfo {
                    title,
                    chapter_id,
                    url: normalize_url(Self::BASE_URL, href),
                });
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
        let url = format!("{}/n/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1.title1");

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("div#content p").unwrap();
        for elem in doc.select(&sel) {
            let txt = element_text(&elem);
            if !txt.is_empty() {
                paragraphs.push(txt);
            }
        }

        let content = clean_content(&paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(Quanben5Provider)
}
