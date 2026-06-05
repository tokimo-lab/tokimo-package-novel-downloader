use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct AkatsukiNovelsProvider;

#[async_trait]
impl Provider for AkatsukiNovelsProvider {
    fn name(&self) -> &str {
        "akatsuki_novels"
    }

    fn display_name(&self) -> &str {
        "暁 - Akatsuki Novels"
    }

    fn base_url(&self) -> &str {
        "https://www.akatsuki-novels.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/stories/index/novel_id~{}", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Title: h3.font-bb a
        info.book_name = select_text(&doc, "h3.font-bb a");

        // Author: second h3.font-bb containing "作者" link
        if let Ok(sel) = Selector::parse("h3.font-bb") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("作者") {
                    if let Ok(a_sel) = Selector::parse("a") {
                        if let Some(a_elem) = elem.select(&a_sel).next() {
                            info.author = element_text(&a_elem);
                        }
                    }
                    break;
                }
            }
        }

        // Summary from the body description div
        if let Ok(sel) = Selector::parse("div.body-x1.body-normal.body-w640 div div") {
            if let Some(elem) = doc.select(&sel).next() {
                info.summary = element_text(&elem);
            }
        }

        // Chapters from table.list tbody tr td a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("table.list tbody tr td:first-child a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    // href format: /stories/view/{chapter_id}/novel_id~{book_id}
                    let chapter_id = href
                        .split("/stories/view/")
                        .nth(1)
                        .and_then(|s| s.split('/').next())
                        .unwrap_or("")
                        .to_string();
                    if chapter_id.is_empty() {
                        continue;
                    }
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
            "{}/stories/view/{}/novel_id~{}",
            self.base_url(),
            chapter_id,
            book_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "h2");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("div.body-novel") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
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
    Box::new(AkatsukiNovelsProvider)
}
