use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct Shu111Provider;

#[async_trait]
impl Provider for Shu111Provider {
    fn name(&self) -> &str {
        "shu111"
    }

    fn display_name(&self) -> &str {
        "书林文学"
    }

    fn base_url(&self) -> &str {
        "http://www.shu111.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("http://www.shu111.com/book/{}.html", book_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1.bookTitle, h1");
        }

        info.author = meta_content(&doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&doc, "p.booktag a[href*=\"authorarticle\"]");
        }

        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&doc, "img.img-thumbnail", "src");
        }

        info.update_time = meta_content(&doc, "og:novel:update_time");
        info.serial_status = meta_content(&doc, "og:novel:status");

        info.summary = meta_content(&doc, "og:description");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "#bookIntro");
        }
        info.summary = info.summary.replace('\u{00a0}', " ");

        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list-chapterAll dd a[href]") {
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
        let url = format!("http://www.shu111.com/book/{}/{}.html", book_id, chapter_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "h1.readTitle, h1").replace('\u{00a0}', " ");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#htmlContent") {
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
    Box::new(Shu111Provider)
}
