use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct ShaoniandreamProvider;

#[async_trait]
impl Provider for ShaoniandreamProvider {
    fn name(&self) -> &str {
        "shaoniandream"
    }

    fn display_name(&self) -> &str {
        "少年之梦"
    }

    fn base_url(&self) -> &str {
        "https://www.shaoniandream.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch book detail page
        let url = format!("{}/book_detail/{}", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&doc, "div.bookdetail-name span.title, div[class*='bookdetail-name'] span.title");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author
        info.author = select_text(&doc, "span.penName a, span[class*='penName'] a");
        if info.author.is_empty() {
            info.author = select_text(&doc, "span.penName, span[class*='penName']");
        }

        // Cover from data-original attribute
        let mut cover = select_attr(&doc, "div.cover img", "data-original");
        if cover.is_empty() {
            cover = select_attr(&doc, "div.cover img", "src");
        }
        if !cover.is_empty() && !cover.starts_with("http") {
            cover = normalize_url(self.base_url(), &cover);
        }
        info.cover_url = cover;

        // Update time
        let update_text = select_text(&doc, "div.bookdetial-newchapter span, div[class*='bookdetial-newchapter'] span");
        info.update_time = update_text.replace("● ", "").trim().to_string();

        // Word count
        if let Ok(sel) = Selector::parse("div.font-list span") {
            if let Some(elem) = doc.select(&sel).next() {
                info.word_count = element_text(&elem);
            }
        }

        // Status
        info.serial_status = select_text(&doc, "div.bookdetail-name i, div[class*='bookdetail-name'] i");

        // Summary
        info.summary = select_text(&doc, "div.bookdetial-jianjie, div[class*='bookdetial-jianjie']");

        // Fetch chapter directory via signing API
        let sign_url = format!(
            "{}/booklibrary/getbookdetaildirsign/book_id/{}",
            self.base_url(),
            book_id
        );
        let sign_resp = client.get(&sign_url).await?;
        let sign_json: serde_json::Value = serde_json::from_str(&sign_resp).unwrap_or_default();

        let access_key = sign_json
            .get("sign")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !access_key.is_empty() {
            let dir_url = format!(
                "{}/booklibrary/getbookdetaildir/BookID/{}",
                self.base_url(),
                book_id
            );
            let form = [("access_key", access_key)];
            let dir_resp = client.post_form(&dir_url, &form).await?;
            let dir_json: serde_json::Value = serde_json::from_str(&dir_resp).unwrap_or_default();

            // Parse chapters from readdir array
            let mut volumes: Vec<Volume> = Vec::new();
            if let Some(readdir) = dir_json.get("readdir").and_then(|v| v.as_array()) {
                for vol_item in readdir {
                    let volume_name = vol_item
                        .get("volume_name")
                        .or_else(|| vol_item.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let mut chapters = Vec::new();
                    if let Some(ch_arr) = vol_item
                        .get("chapters")
                        .or_else(|| vol_item.get("list"))
                        .and_then(|v| v.as_array())
                    {
                        for ch_item in ch_arr {
                            let chapter_id = ch_item
                                .get("chapter_id")
                                .or_else(|| ch_item.get("id"))
                                .and_then(|v| {
                                    v.as_str().map(|s| s.to_string())
                                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                                })
                                .unwrap_or_default();
                            let title = ch_item
                                .get("title")
                                .or_else(|| ch_item.get("chapter_name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            if chapter_id.is_empty() || title.is_empty() {
                                continue;
                            }

                            chapters.push(ChapterInfo {
                                title,
                                chapter_id: chapter_id.clone(),
                                url: format!("{}/read/{}", self.base_url(), chapter_id),
                            });
                        }
                    }

                    if !chapters.is_empty() {
                        volumes.push(Volume {
                            volume_name,
                            chapters,
                        });
                    }
                }
            }

            info.volumes = volumes;
        }

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        // Get chapter signing key
        let sign_url = format!(
            "{}/booklibrary/membersinglechaptersign/chapter_id/{}",
            self.base_url(),
            chapter_id
        );
        let sign_resp = client.get(&sign_url).await?;
        let sign_json: serde_json::Value = serde_json::from_str(&sign_resp).unwrap_or_default();

        let access_key = sign_json
            .get("sign")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let ch_url = format!(
            "{}/booklibrary/membersinglechapter/chapter_id/{}",
            self.base_url(),
            chapter_id
        );
        let form = [("access_key", access_key)];
        let ch_resp = client.post_form(&ch_url, &form).await?;
        let ch_json: serde_json::Value = serde_json::from_str(&ch_resp).unwrap_or_default();

        let mut title = String::new();
        let mut paragraphs = Vec::new();

        if let Some(data) = ch_json.get("data").or(Some(&ch_json)) {
            title = data
                .get("title")
                .or_else(|| data.get("chapter_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Content may be in show_content array (potentially encrypted)
            if let Some(show_content) = data.get("show_content").and_then(|v| v.as_array()) {
                for item in show_content {
                    if let Some(content_str) = item.get("content").and_then(|v| v.as_str()) {
                        // Content may be plain text or encrypted
                        let cleaned = content_str
                            .replace("<i>", "")
                            .replace("</i>", "");
                        let cleaned = strip_html(&cleaned);
                        if !cleaned.trim().is_empty() {
                            paragraphs.push(cleaned.trim().to_string());
                        }
                    }
                }
            }

            // Also check for direct content field
            if paragraphs.is_empty() {
                if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                    let cleaned = html_to_text(content);
                    if !cleaned.is_empty() {
                        paragraphs.push(cleaned);
                    }
                }
            }

            // Postscript
            if let Some(miaoshu) = data.get("miaoshu").and_then(|v| v.as_str()) {
                let cleaned = strip_html(miaoshu).trim().to_string();
                if !cleaned.is_empty() {
                    paragraphs.push(String::new());
                    paragraphs.push(cleaned);
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
    Box::new(ShaoniandreamProvider)
}
