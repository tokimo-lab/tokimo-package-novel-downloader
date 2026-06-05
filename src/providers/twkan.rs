use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 台灣小說網 provider (separate catalog via AJAX)
pub struct TwkanProvider;

#[async_trait]
impl Provider for TwkanProvider {
    fn name(&self) -> &str {
        "twkan"
    }

    fn display_name(&self) -> &str {
        "台灣小說網"
    }

    fn base_url(&self) -> &str {
        "https://twkan.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://twkan.com/book/{}.html", book_id);
        let catalog_url = format!("https://twkan.com/ajax_novels/chapterlist/{}.html", book_id);

        // Fetch info page
        let mut info_headers = HeaderMap::new();
        info_headers.insert("Referer", HeaderValue::from_static("https://twkan.com/"));
        let info_html = client.get_with_headers(&info_url, info_headers).await?;

        // Fetch catalog page
        let referer = format!("https://twkan.com/book/{}/index.html", book_id);
        let mut cata_headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&referer) {
            cata_headers.insert("Referer", val);
        }
        let catalog_html = client.get_with_headers(&catalog_url, cata_headers).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "div.booknav2 h1 a");
        info.author = select_text(&info_doc, "div.booknav2 p a");
        info.cover_url = select_attr(&info_doc, "div.bookimg2 img", "src");

        // Word count & status from "123.45万字 | 连载"
        // Find the paragraph containing "字"
        if let Ok(p_sel) = Selector::parse("div.booknav2 p") {
            for p in info_doc.select(&p_sel) {
                let text = element_text(&p);
                if text.contains('字') && text.contains('|') {
                    let parts: Vec<&str> = text.split('|').collect();
                    if !parts.is_empty() {
                        info.word_count = parts[0].trim().to_string();
                    }
                    if parts.len() > 1 {
                        info.serial_status = parts[1].trim().to_string();
                    }
                    break;
                }
            }
        }

        // Update time
        if let Ok(p_sel) = Selector::parse("div.booknav2 p") {
            for p in info_doc.select(&p_sel) {
                let text = element_text(&p);
                if text.contains("更新") {
                    info.update_time = text.replace("更新：", "").trim().to_string();
                    break;
                }
            }
        }

        info.summary = select_text(&info_doc, "div.navtxt p");

        // Chapters from catalog
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("ul li a[href]") {
            for a in catalog_doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("").trim();
                if href.is_empty() {
                    continue;
                }
                let title = element_text(&a);
                let chap_id = href
                    .trim_matches('/')
                    .rsplit('/')
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
        let url = format!("https://twkan.com/txt/{}/{}", book_id, chapter_id);
        let referer = format!("https://twkan.com/book/{}/index.html", chapter_id);
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&referer) {
            headers.insert("Referer", val);
        }
        let html_text = client.get_with_headers(&url, headers).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "div.txtnav h1");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#txtcontent0") {
            if let Some(content_div) = doc.select(&sel).next() {
                let inner = content_div.inner_html();
                let text = html_to_text(&inner);
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        let mut content = paragraphs.join("\n");
        // Remove title duplication at the start
        let title_trimmed = title.trim();
        if !title_trimmed.is_empty() && content.starts_with(title_trimmed) {
            content = content[title_trimmed.len()..].trim_start().to_string();
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(TwkanProvider)
}
