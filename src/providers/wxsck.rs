use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 万相书城 (wxsck.com) provider with paginated chapters
pub struct WxsckProvider;

impl WxsckProvider {
    fn chapter_url(book_id: &str, chapter_id: &str, page: usize) -> String {
        if page > 1 {
            format!(
                "https://wxsck.com/book/{}/{}_{}.html",
                book_id, chapter_id, page
            )
        } else {
            format!("https://wxsck.com/book/{}/{}.html", book_id, chapter_id)
        }
    }

    fn relative_chapter_url(book_id: &str, chapter_id: &str, page: usize) -> String {
        if page > 1 {
            format!("/book/{}/{}_{}.html", book_id, chapter_id, page)
        } else {
            format!("/book/{}/{}.html", book_id, chapter_id)
        }
    }
}

#[async_trait]
impl Provider for WxsckProvider {
    fn name(&self) -> &str {
        "wxsck"
    }

    fn display_name(&self) -> &str {
        "万相书城"
    }

    fn base_url(&self) -> &str {
        "https://wxsck.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://wxsck.com/book/{}/", book_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = meta_content(&doc, "og:novel:book_name");
        info.author = meta_content(&doc, "og:novel:author");
        info.serial_status = meta_content(&doc, "og:novel:status");
        info.update_time = meta_content(&doc, "og:novel:update_time");

        info.cover_url = meta_content(&doc, "og:image");
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = format!("{}{}", self.base_url(), info.cover_url);
        }

        info.summary = select_text(&doc, "div.book-detail");
        if info.summary.is_empty() {
            info.summary = meta_content(&doc, "og:description");
        }

        // Chapter list
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#all-chapter a[href]") {
            for a in doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("").trim();
                let title = a.text().collect::<Vec<_>>().join("").trim().to_string();
                if href.is_empty() || title.is_empty() {
                    continue;
                }
                let chapter_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
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
        let mut title = String::new();
        let mut all_paragraphs = Vec::new();

        let mut page = 1;
        loop {
            let url = Self::chapter_url(book_id, chapter_id, page);
            let html_text = client.get(&url).await?;
            let doc = Html::parse_document(&html_text);

            if title.is_empty() {
                title = select_text(&doc, "h1.cont-title");
            }

            if let Ok(sel) = Selector::parse("#cont-body p") {
                for p in doc.select(&sel) {
                    let text = element_text(&p);
                    if !text.is_empty() {
                        all_paragraphs.push(text);
                    }
                }
            }

            // Check if next page exists
            page += 1;
            let next_relative = Self::relative_chapter_url(book_id, chapter_id, page);
            if !html_text.contains(&next_relative) {
                break;
            }
            if page > 20 {
                break;
            }
        }

        let content = all_paragraphs.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(WxsckProvider)
}
