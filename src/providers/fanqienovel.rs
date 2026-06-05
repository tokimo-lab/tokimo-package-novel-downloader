use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 番茄小说 (fanqienovel.com) provider.
///
/// Book info and chapter list are embedded as JSON in a
/// `window.__INITIAL_STATE__` script block. Chapter content
/// is also extracted from the same mechanism on reader pages.
/// Font mapping/decryption is skipped for simplicity.
pub struct FanqienovelProvider;

const BASE: &str = "https://fanqienovel.com";

#[async_trait]
impl Provider for FanqienovelProvider {
    fn name(&self) -> &str {
        "fanqienovel"
    }

    fn display_name(&self) -> &str {
        "番茄小说"
    }

    fn base_url(&self) -> &str {
        BASE
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/page/{}", BASE, book_id);
        let html_str = client.get(&url).await?;

        let state = extract_initial_state(&html_str)?;
        let page = state
            .get("page")
            .ok_or_else(|| anyhow::anyhow!("Missing 'page' in __INITIAL_STATE__"))?;

        let mut info = BookInfo::default();

        info.book_name = json_str(page, "bookName");
        info.author = json_str(page, "authorName");
        if info.author.is_empty() {
            info.author = json_str(page, "author");
        }
        info.cover_url = json_str(page, "thumbUrl");
        if info.cover_url.is_empty() {
            info.cover_url = json_str(page, "thumbUri");
        }
        info.update_time = json_str(page, "lastPublishTime");
        info.summary = json_str(page, "abstract");
        if info.summary.is_empty() {
            info.summary = json_str(page, "description");
        }

        // Word count
        if let Some(wn) = page.get("wordNumber") {
            info.word_count = match wn {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
        }

        // Serial status
        let creation_status = page
            .get("creationStatus")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        info.serial_status = if creation_status == 2 {
            "完结".to_string()
        } else {
            "连载".to_string()
        };

        // Volumes and chapters
        let volume_names: Vec<String> = page
            .get("volumeNameList")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let chapter_groups = page.get("chapterListWithVolume").and_then(|v| v.as_array());

        if let Some(groups) = chapter_groups {
            for (i, group) in groups.iter().enumerate() {
                let group_arr = match group.as_array() {
                    Some(a) => a,
                    None => continue,
                };

                let volume_name = if i < volume_names.len() {
                    volume_names[i].clone()
                } else if let Some(first) = group_arr.first() {
                    json_str(first, "volume_name")
                } else {
                    format!("卷 {}", i + 1)
                };
                if volume_name.is_empty() && group_arr.is_empty() {
                    continue;
                }

                // Sort by realChapterOrder or itemId
                let mut sorted_chapters: Vec<&serde_json::Value> = group_arr.iter().collect();
                sorted_chapters.sort_by_key(|ch| {
                    ch.get("realChapterOrder")
                        .or_else(|| ch.get("itemId"))
                        .and_then(|v| match v {
                            serde_json::Value::Number(n) => n.as_i64(),
                            serde_json::Value::String(s) => s.parse::<i64>().ok(),
                            _ => None,
                        })
                        .unwrap_or(0)
                });

                let mut chapters = Vec::new();
                for ch in &sorted_chapters {
                    let chapter_id = json_str(ch, "itemId");
                    let title = json_str(ch, "title");
                    if chapter_id.is_empty() {
                        continue;
                    }
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id: chapter_id.clone(),
                        url: format!("{}/reader/{}", BASE, chapter_id),
                    });
                }

                if !chapters.is_empty() {
                    info.volumes.push(Volume {
                        volume_name: if volume_name.is_empty() {
                            format!("卷 {}", i + 1)
                        } else {
                            volume_name
                        },
                        chapters,
                    });
                }
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
        let url = format!("{}/reader/{}", BASE, chapter_id);
        let html_str = client.get(&url).await?;

        let state = extract_initial_state(&html_str)?;

        let chapter_data = state.get("reader").and_then(|r| r.get("chapterData"));

        let (title, content) = if let Some(cd) = chapter_data {
            let title = json_str(cd, "title");
            let raw_content = json_str(cd, "content");

            if raw_content.is_empty() {
                (title, "[章节内容为空]".to_string())
            } else {
                // Parse the content HTML and extract paragraphs
                let content_doc = Html::parse_fragment(&raw_content);
                let mut paragraphs = Vec::new();
                if let Ok(p_sel) = Selector::parse("p") {
                    for p in content_doc.select(&p_sel) {
                        let text = element_text(&p).trim().to_string();
                        if !text.is_empty() {
                            paragraphs.push(text);
                        }
                    }
                }
                // If no <p> tags, try plain text
                if paragraphs.is_empty() {
                    let text = html_to_text(&raw_content);
                    if !text.trim().is_empty() {
                        paragraphs.push(text.trim().to_string());
                    }
                }
                let content = if paragraphs.is_empty() {
                    "[无法解析章节内容]".to_string()
                } else {
                    paragraphs.join("\n")
                };
                (title, content)
            }
        } else {
            // Fallback: try to parse from HTML directly
            let doc = Html::parse_document(&html_str);
            let title = select_text(&doc, "h1");
            let mut content = String::new();
            for sel_str in &["div.chapter-content", "div.read-content", "article"] {
                if let Ok(sel) = Selector::parse(sel_str) {
                    if let Some(elem) = doc.select(&sel).next() {
                        content = html_to_text(&elem.inner_html());
                        if !content.is_empty() {
                            break;
                        }
                    }
                }
            }
            if content.is_empty() {
                content = "[无法获取章节内容]".to_string();
            }
            (title, content)
        };

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

/// Extract the `window.__INITIAL_STATE__` JSON object from the HTML.
fn extract_initial_state(html_str: &str) -> Result<serde_json::Value> {
    let marker = "window.__INITIAL_STATE__";
    let start = html_str
        .find(marker)
        .ok_or_else(|| anyhow::anyhow!("__INITIAL_STATE__ not found in HTML"))?;

    let rest = &html_str[start..];

    // Find the = sign
    let eq = rest
        .find('=')
        .ok_or_else(|| anyhow::anyhow!("No '=' after __INITIAL_STATE__"))?;

    let after_eq = &rest[eq + 1..];

    // Find the opening brace
    let brace = after_eq
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("No '{{' found after __INITIAL_STATE__ ="))?;

    let json_start = &after_eq[brace..];

    // Find the end: look for }; or } followed by </script>
    // Use brace counting for balanced extraction
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let mut end_pos = 0;

    for (i, ch) in json_start.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if end_pos == 0 {
        return Err(anyhow::anyhow!(
            "Could not find end of __INITIAL_STATE__ JSON"
        ));
    }

    let json_str = &json_start[..end_pos];

    // Try standard JSON parse first
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        return Ok(val);
    }

    // Fallback: the JS object might have unquoted keys or single-quoted strings.
    // Try a lenient approach: replace common JS patterns
    let cleaned = json_str.replace("undefined", "null").replace("'", "\"");
    serde_json::from_str::<serde_json::Value>(&cleaned)
        .map_err(|e| anyhow::anyhow!("Failed to parse __INITIAL_STATE__ JSON: {}", e))
}

/// Helper to extract a string field from a JSON value.
fn json_str(val: &serde_json::Value, key: &str) -> String {
    match val.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(FanqienovelProvider)
}
