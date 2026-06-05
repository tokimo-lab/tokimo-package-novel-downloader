use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 一笔阁 (yibige.org) provider with separate catalog page
pub struct YibigeProvider;

const BASE_HOST: &str = "www.yibige.org";

#[async_trait]
impl Provider for YibigeProvider {
    fn name(&self) -> &str {
        "yibige"
    }

    fn display_name(&self) -> &str {
        "一笔阁"
    }

    fn base_url(&self) -> &str {
        "https://www.yibige.org"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://{}/{}/", BASE_HOST, book_id);
        let catalog_url = format!("https://{}/{}/index.html", BASE_HOST, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        // Metadata from og: meta tags
        info.book_name = meta_content(&info_doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "#info h1");
        }

        info.author = meta_content(&info_doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&info_doc, "#info p a");
        }

        info.cover_url = meta_content(&info_doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&info_doc, "#fmimg img", "src");
        }

        info.update_time = meta_content(&info_doc, "og:novel:update_time").replace('T', " ");
        info.serial_status = meta_content(&info_doc, "og:novel:status");
        if info.serial_status.is_empty() {
            info.serial_status = "连载中".to_string();
        }

        // Word count from info p containing "字数："
        if let Ok(p_sel) = Selector::parse("#info p") {
            for p in info_doc.select(&p_sel) {
                let text = element_text(&p);
                if text.contains("字数：") {
                    info.word_count = text.replace("字数：", "").trim().to_string();
                    break;
                }
            }
        }

        info.summary = select_text(&info_doc, "#intro p");

        // Chapters from catalog page
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list dl dd a[href]") {
            for a in catalog_doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("").trim();
                if href.is_empty() {
                    continue;
                }
                let title = element_text(&a);
                if title.is_empty() {
                    continue;
                }
                let chap_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                chapters.push(ChapterInfo {
                    title,
                    chapter_id: chap_id,
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
        let url = format!("https://{}/{}/{}.html", BASE_HOST, book_id, chapter_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "div.bookname h1");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#content p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                let normalized = text.replace('\u{00a0}', " ").replace('\u{3000}', "  ");
                let trimmed = normalized.trim().to_string();
                if !trimmed.is_empty() {
                    paragraphs.push(trimmed);
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
    Box::new(YibigeProvider)
}
