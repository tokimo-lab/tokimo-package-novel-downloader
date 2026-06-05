use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 69书吧 provider (separate catalog, reversed chapter order)
pub struct N69shubaProvider;

#[async_trait]
impl Provider for N69shubaProvider {
    fn name(&self) -> &str {
        "n69shuba"
    }

    fn display_name(&self) -> &str {
        "69书吧"
    }

    fn base_url(&self) -> &str {
        "https://www.69shuba.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://www.69shuba.com/book/{}.htm", book_id);
        let catalog_url = format!("https://www.69shuba.com/book/{}/", book_id);

        let mut headers = HeaderMap::new();
        headers.insert(
            "Referer",
            HeaderValue::from_static("https://www.69shuba.com/"),
        );

        let info_html = client.get_with_headers(&info_url, headers.clone()).await?;
        let catalog_html = client.get_with_headers(&catalog_url, headers).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "div.booknav2 h1 a");
        info.author = select_text(&info_doc, "div.booknav2 p a");
        info.cover_url = select_attr(&info_doc, "div.bookimg2 img", "src");

        // Word count & status from "123.45万字 | 连载"
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

        // Chapters from catalog (reversed - newest first)
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#catalog ul li a[href]") {
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

        // Reverse to chronological order
        chapters.reverse();

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
        let url = format!("https://www.69shuba.com/txt/{}/{}", book_id, chapter_id);
        let referer = format!("https://www.69shuba.com/book/{}/", chapter_id);
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&referer) {
            headers.insert("Referer", val);
        }
        let html_text = client.get_with_headers(&url, headers).await?;
        let doc = Html::parse_document(&html_text);

        let mut title = String::new();
        let mut paragraphs: Vec<String> = Vec::new();

        // Parse direct children of div.txtnav
        if let Ok(sel) = Selector::parse("div.txtnav > *") {
            for elem in doc.select(&sel) {
                let tag = elem.value().name();
                let cls = elem.value().attr("class").unwrap_or("").to_lowercase();
                let eid = elem.value().attr("id").unwrap_or("").to_lowercase();

                // Title
                if tag == "h1" && title.is_empty() {
                    title = element_text(&elem);
                    continue;
                }

                // Skip metadata/ads
                if cls.contains("txtinfo") || cls.contains("bottom-ad") || eid.contains("txtright")
                {
                    continue;
                }

                // Regular text
                let text = element_text(&elem);
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        // Remove title duplication
        if !paragraphs.is_empty() && paragraphs[0].trim() == title.trim() {
            paragraphs.remove(0);
        }

        // Remove chapter footer marker
        if let Some(last) = paragraphs.last_mut() {
            if last.ends_with("(本章完)") {
                let trimmed = last[..last.len() - "(本章完)".len()].trim_end().to_string();
                if trimmed.is_empty() {
                    paragraphs.pop();
                } else {
                    *last = trimmed;
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
    Box::new(N69shubaProvider)
}
