use anyhow::Result;
use async_trait::async_trait;

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct KadokadoProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(KadokadoProvider)
}

const BASE_URL: &str = "https://www.kadokado.com.tw";
const API_BASE: &str = "https://api.kadokado.com.tw";

#[async_trait]
impl Provider for KadokadoProvider {
    fn name(&self) -> &str {
        "kadokado"
    }

    fn display_name(&self) -> &str {
        "KadoKado"
    }

    fn base_url(&self) -> &str {
        BASE_URL
    }

    fn supports_search(&self) -> bool {
        true
    }

    async fn search(
        &self,
        client: &HttpClient,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let url = format!(
            "{}/v3/search?order=Relevance&typeFilter=All&statusFilter=All&rRatedFilter=All&paidContentFilter=All&wordCountFilter=All&keyword={}&current=1&limit=96",
            API_BASE,
            urlencoding::encode(keyword)
        );
        let resp = client.inner().get(&url).send().await?;
        let text = resp.text().await?;
        let data: serde_json::Value = serde_json::from_str(&text)?;

        let mut results = Vec::new();
        if let Some(rows) = data.get("data").and_then(|d| d.as_array()) {
            for row in rows.iter().take(limit) {
                let book_id = row
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .or_else(|| {
                        row.get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                if book_id.is_empty() {
                    continue;
                }
                let title = row
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let author = row
                    .get("ownerDisplayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let cover_urls = row.get("coverUrls").and_then(|v| v.as_array());
                let _cover_url = cover_urls
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let word_count = row
                    .get("wordCount")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date: String::new(),
                    word_count,
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch info and catalog in sequence
        let info_url = format!("{}/v2/titles/{}", API_BASE, book_id);
        let catalog_url = format!("{}/v3/title/{}/collection", API_BASE, book_id);

        let info_resp = client.inner().get(&info_url).send().await?;
        let info_text = info_resp.text().await?;
        let info_data: serde_json::Value = serde_json::from_str(&info_text)?;

        let catalog_resp = client.inner().get(&catalog_url).send().await?;
        let catalog_text = catalog_resp.text().await?;
        let catalog_data: serde_json::Value = serde_json::from_str(&catalog_text)?;

        let mut info = BookInfo::default();
        info.book_name = info_data
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        info.author = info_data
            .get("ownerDisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        info.summary = info_data
            .get("logline")
            .or_else(|| info_data.get("oneLineIntro"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        info.cover_url = info_data
            .get("coverUrls")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        info.word_count = info_data
            .get("wordCount")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string())
            .unwrap_or_default();

        // Parse catalog
        if let Some(vols) = catalog_data.as_array() {
            for (idx, vol) in vols.iter().enumerate() {
                let vol_name = vol
                    .get("collectionDisplayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let vol_name = if vol_name.is_empty() {
                    format!("未命名卷 {}", idx + 1)
                } else {
                    vol_name
                };

                let mut chapters = Vec::new();
                if let Some(chs) = vol.get("chapters").and_then(|v| v.as_array()) {
                    for ch in chs {
                        let chap_id = ch
                            .get("chapterId")
                            .and_then(|v| v.as_i64())
                            .map(|v| v.to_string())
                            .or_else(|| {
                                ch.get("chapterId")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_default();
                        let title = ch
                            .get("chapterDisplayName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id: chap_id.clone(),
                            url: format!("{}/chapter/{}", BASE_URL, chap_id),
                        });
                    }
                }

                info.volumes.push(Volume {
                    volume_name: vol_name,
                    chapters,
                });
            }
        }

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        // Fetch chapter info and content
        let info_url = format!("{}/v3/chapter/{}/info", API_BASE, chapter_id);
        let content_url = format!("{}/v3/chapter/{}/content", API_BASE, chapter_id);

        let info_resp = client.inner().get(&info_url).send().await?;
        let info_text = info_resp.text().await?;
        let info_data: serde_json::Value = serde_json::from_str(&info_text)?;

        let content_resp = client.inner().get(&content_url).send().await?;
        let content_text = content_resp.text().await?;
        let content_data: serde_json::Value = serde_json::from_str(&content_text)?;

        let title = info_data
            .get("chapterDisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let raw_content = content_data
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Parse HTML content
        let content = if !raw_content.is_empty() {
            let doc = scraper::Html::parse_fragment(raw_content);
            let mut paragraphs = Vec::new();
            if let Ok(sel) = scraper::Selector::parse("p") {
                for elem in doc.select(&sel) {
                    let text = element_text(&elem);
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                }
            }
            if paragraphs.is_empty() {
                html_to_text(raw_content)
            } else {
                paragraphs.join("\n")
            }
        } else {
            String::new()
        };

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}
