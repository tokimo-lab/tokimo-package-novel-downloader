use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 镇魂小说网 provider
pub struct ZhenhunxiaoshuoProvider;

#[async_trait]
impl Provider for ZhenhunxiaoshuoProvider {
    fn name(&self) -> &str {
        "zhenhunxiaoshuo"
    }

    fn display_name(&self) -> &str {
        "镇魂小说网"
    }

    fn base_url(&self) -> &str {
        "https://www.zhenhunxiaoshuo.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.zhenhunxiaoshuo.com/{}/", book_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "h1.focusbox-title");

        // Summary from focusbox-text p.text
        info.summary = select_text(&doc, "div.focusbox-text p.text")
            .replace('\u{3000}', " ");

        // Chapter list from excerpts article a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.excerpts article a[href]") {
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
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "https://www.zhenhunxiaoshuo.com/{}.html",
            chapter_id
        );
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "header.article-header h1.article-title");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("article.article-content p") {
            for p in doc.select(&sel) {
                let text = p.text().collect::<Vec<_>>().join("").trim().to_string();
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        let content = paragraphs.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(ZhenhunxiaoshuoProvider)
}
