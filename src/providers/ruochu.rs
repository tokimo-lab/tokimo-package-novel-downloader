use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct RuochuProvider;

#[async_trait]
impl Provider for RuochuProvider {
    fn name(&self) -> &str {
        "ruochu"
    }

    fn display_name(&self) -> &str {
        "若初文学网"
    }

    fn base_url(&self) -> &str {
        "https://www.ruochu.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch book detail page
        let info_url = format!("{}/book/{}", self.base_url(), book_id);
        let html_str = client.get(&info_url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&doc, "div.pattern-cover-detail h1 span, div[class*='pattern-cover-detail'] h1 span");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author from notify div
        if let Ok(sel) = Selector::parse("div.notify, div[class*='notify']") {
            if let Some(elem) = doc.select(&sel).next() {
                let text = element_text(&elem);
                if text.contains("作者") {
                    // Extract author name after "作者"
                    if let Some(pos) = text.find("作者") {
                        let after = &text[pos + "作者".len()..];
                        let after = after.trim_start_matches('：').trim_start_matches(':').trim();
                        // Take until next whitespace or special char
                        info.author = after
                            .split_whitespace()
                            .next()
                            .unwrap_or(after)
                            .to_string();
                    }
                }
            }
        }

        // Cover
        info.cover_url = select_attr(&doc, "div.pic img.book-cover, img[class*='book-cover']", "src");
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = normalize_url(self.base_url(), &info.cover_url);
        }

        // Word count
        info.word_count = select_text(&doc, "span.words, span[class*='words']");

        // Status
        if let Ok(sel) = Selector::parse("i.is-serialize, i[class*='is-serialize']") {
            if doc.select(&sel).next().is_some() {
                info.serial_status = "连载中".to_string();
            } else {
                info.serial_status = "完结".to_string();
            }
        }

        // Update time
        info.update_time = select_text(&doc, "span.time, span[class*='time']");

        // Summary
        info.summary = select_text(&doc, "div.summary pre.note, div[class*='summary'] pre.note");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "div.summary, div[class*='summary']");
        }

        // Fetch chapter list from catalog page
        let catalog_url = format!("{}/chapter/{}", self.base_url(), book_id);
        let catalog_html = client.get(&catalog_url).await?;
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.chapter-list ul li a[href], div[class*='chapter-list'] ul li a[href]") {
            for elem in catalog_doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .split('?')
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
        }

        if !chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: String::new(),
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
        // Use JSONP API to fetch chapter content
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let callback = format!("jQuery{}_{}", fastrand_simple(), ts);
        let url = format!(
            "https://a.ruochu.com/ajax/chapter/content/{}?callback={}",
            chapter_id, callback
        );

        let resp = client.get(&url).await?;

        // Parse JSONP response: callback({...})
        let json_str = if let Some(start) = resp.find('(') {
            let end = resp.rfind(')').unwrap_or(resp.len());
            &resp[start + 1..end]
        } else {
            &resp
        };

        let json: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();

        let mut title = String::new();
        let mut content = String::new();

        if let Some(chapter) = json.get("chapter").or_else(|| json.get("data")) {
            title = chapter
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let html_content = chapter
                .get("htmlContent")
                .or_else(|| chapter.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if !html_content.is_empty() {
                let doc = Html::parse_fragment(html_content);
                let mut paragraphs = Vec::new();
                if let Ok(sel) = Selector::parse("p") {
                    for elem in doc.select(&sel) {
                        let text = element_text(&elem);
                        if !text.is_empty() {
                            paragraphs.push(text);
                        }
                    }
                }
                if paragraphs.is_empty() {
                    content = html_to_text(html_content);
                } else {
                    content = paragraphs.join("\n");
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

/// Simple pseudo-random number generator for JSONP callback names
fn fastrand_simple() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}", ts % 10_000_000_000_000_000)
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(RuochuProvider)
}
