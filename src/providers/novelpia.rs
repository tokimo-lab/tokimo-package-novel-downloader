use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct NovelpiaProvider;

impl NovelpiaProvider {
    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[async_trait]
impl Provider for NovelpiaProvider {
    fn name(&self) -> &str {
        "novelpia"
    }

    fn display_name(&self) -> &str {
        "ノベルピア"
    }

    fn base_url(&self) -> &str {
        "https://novelpia.jp"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch novel info via API
        let ts = Self::timestamp();
        let url = format!(
            "{}/proc/novel?cmd=get_novel&novel_no={}&mem_nick=HATI&_={}",
            self.base_url(),
            book_id,
            ts
        );
        let json_str = client.get(&url).await?;
        let json: serde_json::Value = serde_json::from_str(&json_str)?;

        let mut info = BookInfo::default();

        if let Some(novel) = json.get("novel") {
            info.book_name = novel
                .get("novel_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            info.author = novel
                .get("writer_nick")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Cover
            let cover = novel
                .get("cover_img")
                .or_else(|| novel.get("novel_img_all"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if cover.starts_with("//") {
                info.cover_url = format!("https:{}", cover);
            } else {
                info.cover_url = cover;
            }

            info.update_time = novel
                .get("last_write_date")
                .or_else(|| novel.get("status_date"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Summary
            info.summary = novel
                .get("novel_story")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n");
            info.summary = strip_html(&info.summary);
        }

        // Fetch episode list via POST
        let total_count = json
            .get("novel")
            .and_then(|n| n.get("count_book"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let per_page = 20;
        let total_pages = if total_count > 0 {
            (total_count + per_page - 1) / per_page
        } else {
            1
        };

        let mut chapters = Vec::new();
        for page in 0..total_pages {
            let ep_url = format!("{}/proc/episode_list", self.base_url());
            let form = [
                ("novel_no", book_id.to_string()),
                ("page", page.to_string()),
                ("sort", "1".to_string()),
            ];
            let form_refs: Vec<(&str, &str)> = form
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();
            let ep_html = client.post_form(&ep_url, &form_refs).await?;
            let doc = Html::parse_document(&ep_html);

            if let Ok(sel) = Selector::parse("tr[class*='ep_style5']") {
                for elem in doc.select(&sel) {
                    let content_no = elem
                        .value()
                        .attr("data-content-no")
                        .unwrap_or("")
                        .to_string();
                    if content_no.is_empty() {
                        continue;
                    }

                    // Title from b tag or text
                    let mut title = String::new();
                    if let Ok(b_sel) = Selector::parse("b") {
                        if let Some(b_elem) = elem.select(&b_sel).next() {
                            title = element_text(&b_elem);
                        }
                    }
                    if title.is_empty() {
                        if let Ok(td_sel) = Selector::parse("td") {
                            if let Some(td) = elem.select(&td_sel).next() {
                                title = element_text(&td);
                            }
                        }
                    }
                    if title.is_empty() {
                        title = format!("Episode {}", content_no);
                    }

                    chapters.push(ChapterInfo {
                        title,
                        chapter_id: content_no.clone(),
                        url: format!("{}/viewer/{}", self.base_url(), content_no),
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
        let url = format!("{}/proc/viewer_data/{}", self.base_url(), chapter_id);
        let form = [("no", chapter_id)];
        let json_str = client.post_form(&url, &form).await?;
        let json: serde_json::Value = serde_json::from_str(&json_str)?;

        let mut title = String::new();
        let mut paragraphs = Vec::new();

        // Parse content from "s" array
        if let Some(s_arr) = json.get("s").and_then(|v| v.as_array()) {
            for item in s_arr {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if text.is_empty() {
                        continue;
                    }
                    // Parse HTML content
                    let doc = Html::parse_fragment(text);
                    if let Ok(sel) = Selector::parse("p") {
                        for p_elem in doc.select(&sel) {
                            let p_text = element_text(&p_elem);
                            if !p_text.is_empty() {
                                paragraphs.push(p_text);
                            }
                        }
                    }
                    // If no p tags, use raw text
                    if paragraphs.is_empty() {
                        let cleaned = html_to_text(text);
                        if !cleaned.is_empty() {
                            paragraphs.push(cleaned);
                        }
                    }
                }
            }
        }

        // Try to get title from JSON
        if let Some(t) = json.get("title").and_then(|v| v.as_str()) {
            title = t.to_string();
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
    Box::new(NovelpiaProvider)
}
