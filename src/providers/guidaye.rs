use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct GuidayeProvider;

impl GuidayeProvider {
    fn normalize_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for GuidayeProvider {
    fn name(&self) -> &str {
        "guidaye"
    }

    fn display_name(&self) -> &str {
        "名著阅读"
    }

    fn base_url(&self) -> &str {
        "https://b.guidaye.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let normalized = Self::normalize_book_id(book_id);
        let url = format!("{}/{}/", self.base_url(), normalized);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // All from og:* meta tags
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        info.author = meta_content(&doc, "og:novel:author");
        info.cover_url = meta_content(&doc, "og:image");
        info.update_time = meta_content(&doc, "og:novel:update_time");
        info.summary = meta_content(&doc, "og:description");

        // Chapters from ol#list-ol a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("ol#list-ol a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
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
        let normalized = Self::normalize_book_id(book_id);
        let url = format!(
            "{}/{}/{}.html",
            self.base_url(),
            normalized,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        // Title from h1 with class secondfont or entry-title
        let mut title = select_text(&doc, "h1.secondfont");
        if title.is_empty() {
            title = select_text(&doc, "h1.entry-title");
        }
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        // Content from article.article-post
        let mut content = String::new();
        if let Ok(sel) = Selector::parse("article.article-post") {
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
    Box::new(GuidayeProvider)
}
