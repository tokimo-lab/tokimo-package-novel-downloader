use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 西方奇幻小说网 provider
pub struct WestnovelProvider;

impl WestnovelProvider {
    fn transform_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for WestnovelProvider {
    fn name(&self) -> &str {
        "westnovel"
    }

    fn display_name(&self) -> &str {
        "西方奇幻小说网"
    }

    fn base_url(&self) -> &str {
        "https://www.westnovel.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let real_id = Self::transform_book_id(book_id);
        let url = format!("https://www.westnovel.com/{}/", real_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "div.btitle h1 a");
        info.author = select_text(&doc, "div.btitle em")
            .replace("作者：", "");
        info.author = info.author.trim().to_string();

        let cover_path = select_attr(&doc, "div.bookinfo img.img-img", "src");
        if !cover_path.is_empty() {
            info.cover_url = format!("{}{}", self.base_url(), cover_path);
        }

        info.summary = select_text(&doc, "div.intro span.intro-p p")
            .replace("内容简介：", "");

        // Chapter list
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("dl.chapterlist dd a[href]") {
            for a in doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("").trim();
                let title = element_text(&a);
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
        let real_id = Self::transform_book_id(book_id);
        let url = format!(
            "https://www.westnovel.com/{}/{}.html",
            real_id, chapter_id
        );
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "#BookCon h1");

        // Try paragraph extraction first, fallback to raw text
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#BookText p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        if paragraphs.is_empty() {
            if let Ok(sel) = Selector::parse("#BookText") {
                if let Some(elem) = doc.select(&sel).next() {
                    let text = html_to_text(&elem.inner_html());
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
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
    Box::new(WestnovelProvider)
}
