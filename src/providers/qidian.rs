use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::select_text_in;
use crate::types::*;
use crate::utils::*;

/// 起点中文网 (www.qidian.com) provider.
///
/// Supports search and basic (free/non-encrypted) chapter reading.
/// Complex RC4 cookie tokens and font encryption are skipped;
/// only plaintext chapter content is extracted.
pub struct QidianProvider;

const BASE: &str = "https://www.qidian.com";

#[async_trait]
impl Provider for QidianProvider {
    fn name(&self) -> &str {
        "qidian"
    }

    fn display_name(&self) -> &str {
        "起点中文网"
    }

    fn base_url(&self) -> &str {
        BASE
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
            "https://www.qidian.com/so/{}.html",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse(r#"div#result-list li.res-book-item"#) {
            for elem in doc.select(&sel).take(limit) {
                let book_id = elem.value().attr("data-bid").unwrap_or("").to_string();
                if book_id.is_empty() {
                    continue;
                }

                let title = select_text_in(&elem, "h3 a");
                let author = select_text_in(&elem, "p.author a.name, p.author i");
                let latest_chapter = select_text_in(&elem, "p.update a");
                let update_date = select_text_in(&elem, "p.update span");
                let word_count = select_text_in(&elem, "div.book-right-info div.total p span");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date,
                    word_count,
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/book/{}/", BASE, book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&doc, r#"h1#bookName"#);
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:novel:book_name");
        }
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }

        // Author
        info.author = select_text(&doc, "a.writer-name");
        if info.author.is_empty() {
            info.author = meta_content(&doc, "og:novel:author");
        }

        // Cover
        info.cover_url = format!(
            "https://bookcover.yuewen.com/qdbimg/349573/{}/600.webp",
            book_id
        );

        // Update time
        info.update_time = select_text(&doc, "span.update-time")
            .replace("更新时间:", "")
            .trim()
            .to_string();
        if info.update_time.is_empty() {
            info.update_time = meta_content(&doc, "og:novel:update_time");
        }

        // Serial status
        info.serial_status = select_text(&doc, "p.book-attribute span");
        if info.serial_status.is_empty() {
            info.serial_status = meta_content(&doc, "og:novel:status");
        }

        // Word count
        info.word_count = select_text(&doc, "p.count em");

        // Summary
        info.summary = select_text(&doc, "p#book-intro-detail");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "p.intro");
        }
        if info.summary.is_empty() {
            info.summary = meta_content(&doc, "og:description");
        }

        // Volumes and chapters from #allCatalog
        if let Ok(vol_sel) = Selector::parse(r#"div#allCatalog div.catalog-volume"#) {
            for vol_elem in doc.select(&vol_sel) {
                let mut vol_name = select_text_in(&vol_elem, "h3.volume-name");
                // Strip text after middle dot
                if let Some(idx) = vol_name.find('\u{00B7}') {
                    vol_name = vol_name[..idx].trim().to_string();
                }

                let mut chapters = Vec::new();
                if let Ok(ch_sel) = Selector::parse("ul.volume-chapters li a.chapter-name") {
                    for a_elem in vol_elem.select(&ch_sel) {
                        let title = element_text(&a_elem);
                        let href = a_elem.value().attr("href").unwrap_or("").to_string();
                        let chapter_id = href
                            .trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        if title.is_empty() || chapter_id.is_empty() {
                            continue;
                        }
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id: chapter_id.clone(),
                            url: format!("{}/chapter/{}/{}/", BASE, book_id, chapter_id),
                        });
                    }
                }
                if !chapters.is_empty() {
                    info.volumes.push(Volume {
                        volume_name: vol_name,
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
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/chapter/{}/{}/", BASE, book_id, chapter_id);
        let html_str = client.get(&url).await?;

        // Try to extract SSR JSON from vite-plugin-ssr_pageContext script tag
        let title;
        let content;

        if let Some(chapter_info) = extract_ssr_chapter_info(&html_str) {
            title = chapter_info
                .get("chapterName")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();

            // Skip VIP/encrypted chapters
            let vip_status = chapter_info
                .get("vipStatus")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let is_buy = chapter_info
                .get("isBuy")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if vip_status == 1 && is_buy == 0 {
                return Ok(Chapter {
                    id: chapter_id.to_string(),
                    title,
                    content: "[VIP章节，需要订阅]".to_string(),
                });
            }

            let raw_html = chapter_info
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Parse paragraphs from <p> tags in the content HTML
            let mut paragraphs = Vec::new();
            for part in raw_html.split("<p>") {
                let cleaned = part.replace("</p>", "").trim().to_string();
                if !cleaned.is_empty() {
                    let text = strip_html(&cleaned);
                    let unescaped = html_escape_decode(&text);
                    if !unescaped.trim().is_empty() {
                        paragraphs.push(unescaped.trim().to_string());
                    }
                }
            }
            content = paragraphs.join("\n");
        } else {
            // Fallback: parse as regular HTML
            let doc = Html::parse_document(&html_str);
            title = select_text(&doc, "h1");

            let mut raw = String::new();
            for sel_str in &["#chapter-content", "#content", "article", ".read-content"] {
                if let Ok(sel) = Selector::parse(sel_str) {
                    if let Some(elem) = doc.select(&sel).next() {
                        raw = elem.inner_html();
                        break;
                    }
                }
            }
            content = html_to_text(&raw);
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

/// Attempt to extract the chapterInfo object from the SSR page context JSON.
fn extract_ssr_chapter_info(html_str: &str) -> Option<serde_json::Value> {
    let marker = r#"id="vite-plugin-ssr_pageContext""#;
    let start = html_str.find(marker)?;
    let rest = &html_str[start..];

    // Find the opening > of the script tag
    let gt = rest.find('>')?;
    let after_tag = &rest[gt + 1..];

    // Find the closing </script>
    let end = after_tag.find("</script>")?;
    let json_str = after_tag[..end].trim();

    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let chapter_info = parsed
        .get("pageContext")?
        .get("pageProps")?
        .get("pageData")?
        .get("chapterInfo")?
        .clone();
    Some(chapter_info)
}

/// Basic HTML entity decoding for common entities.
fn html_escape_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(QidianProvider)
}
