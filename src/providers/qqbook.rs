use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::Html;

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// QQ阅读 provider (book.qq.com)
/// Note: Chapter content may be encrypted; this implementation handles
/// the basic (non-encrypted) case.
pub struct QqbookProvider;

impl QqbookProvider {
    fn extract_nuxt_data(html: &str) -> Option<serde_json::Value> {
        let re = Regex::new(r"window\.__NUXT__\s*=\s*([\s\S]*?);\s*</script>").ok()?;
        let cap = re.captures(html)?;
        let js_str = cap.get(1)?.as_str().trim();

        // Try parsing as JSON directly (works for simple cases)
        // The NUXT block is JavaScript, not pure JSON, so we do best-effort parsing
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(js_str) {
            return Some(val);
        }

        // Fallback: try to extract data array manually
        None
    }
}

#[async_trait]
impl Provider for QqbookProvider {
    fn name(&self) -> &str {
        "qqbook"
    }

    fn display_name(&self) -> &str {
        "QQ阅读"
    }

    fn base_url(&self) -> &str {
        "https://book.qq.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://book.qq.com/book-detail/{}", book_id);
        let catalog_url = format!(
            "https://book.qq.com/api/book/detail/chapters?bid={}",
            book_id
        );

        let info_html = client.get(&info_url).await?;
        let catalog_json = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);

        let mut info = BookInfo::default();

        // Book metadata from meta tags
        info.book_name = meta_content(&info_doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "h1.book-title");
        }

        info.author = meta_content(&info_doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&info_doc, "div.book-meta a.author");
        }
        info.author = info.author.replace(" 著", "").replace("著", "");

        info.cover_url = meta_content(&info_doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&info_doc, "div.book-cover img", "src");
        }

        info.update_time = meta_content(&info_doc, "og:novel:update_time");
        info.serial_status = meta_content(&info_doc, "og:novel:status");

        info.summary = select_text(&info_doc, "div.book-intro");
        if info.summary.is_empty() {
            info.summary = meta_content(&info_doc, "og:description");
        }

        // Extract book_id from read URL for chapter URL construction
        let read_url = meta_content(&info_doc, "og:novel:read_url");
        let effective_book_id = if !read_url.is_empty() {
            read_url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(book_id)
                .to_string()
        } else {
            book_id.to_string()
        };

        // Parse catalog from API JSON
        let mut chapters = Vec::new();
        if let Ok(catalog) = serde_json::from_str::<serde_json::Value>(&catalog_json) {
            if let Some(data) = catalog.get("data").and_then(|d| d.as_array()) {
                for item in data {
                    let cid = item
                        .get("cid")
                        .map(|v| {
                            if let Some(n) = v.as_u64() {
                                n.to_string()
                            } else {
                                v.as_str().unwrap_or("").to_string()
                            }
                        })
                        .unwrap_or_default();
                    let title = item
                        .get("chapterName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if cid.is_empty() || title.is_empty() {
                        continue;
                    }
                    let url = format!("/book-read/{}/{}", effective_book_id, cid);
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id: cid,
                        url: normalize_url(self.base_url(), &url),
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
        let url = format!("https://book.qq.com/book-read/{}/{}/", book_id, chapter_id);
        let html_text = client.get(&url).await?;

        // Try to extract from NUXT data
        if let Some(nuxt) = Self::extract_nuxt_data(&html_text) {
            if let Some(data_arr) = nuxt.get("data").and_then(|d| d.as_array()) {
                if let Some(data_block) = data_arr.first() {
                    let title = data_block
                        .get("chapterTitle")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Untitled")
                        .to_string();
                    let cid = data_block
                        .get("cid")
                        .map(|v| {
                            if let Some(n) = v.as_u64() {
                                n.to_string()
                            } else {
                                v.as_str().unwrap_or(chapter_id).to_string()
                            }
                        })
                        .unwrap_or_else(|| chapter_id.to_string());

                    let content = data_block
                        .get("currentContent")
                        .and_then(|cc| cc.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();

                    let is_encrypted = data_block
                        .get("currentContent")
                        .and_then(|cc| cc.get("encrypt"))
                        .and_then(|e| e.as_bool())
                        .unwrap_or(false);

                    if is_encrypted {
                        return Ok(Chapter {
                            id: cid,
                            title,
                            content: "[内容已加密，需要特殊解密处理]".to_string(),
                        });
                    }

                    // Parse HTML content
                    let cleaned = html_to_text(&content);
                    return Ok(Chapter {
                        id: cid,
                        title,
                        content: cleaned,
                    });
                }
            }
        }

        // Fallback: parse from HTML structure
        let doc = Html::parse_document(&html_text);
        let title = select_text(&doc, "h1");
        let content = select_text(&doc, "div.chapter-content, #content, article");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(QqbookProvider)
}
