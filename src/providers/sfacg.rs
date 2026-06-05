use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// SF轻小说 (m.sfacg.com) provider.
///
/// Uses the mobile site for simpler HTML structure.
/// VIP chapters (image-based) are detected but not decoded.
pub struct SfacgProvider;

const BASE: &str = "https://m.sfacg.com";

#[async_trait]
impl Provider for SfacgProvider {
    fn name(&self) -> &str {
        "sfacg"
    }

    fn display_name(&self) -> &str {
        "SF轻小说"
    }

    fn base_url(&self) -> &str {
        BASE
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/b/{}/", BASE, book_id);
        let catalog_url = format!("{}/i/{}/", BASE, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&info_doc, "span.book_newtitle");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "h1");
        }

        // Author, word count from book_info3 span
        if let Ok(sel) = Selector::parse("span.book_info3") {
            if let Some(elem) = info_doc.select(&sel).next() {
                let text = element_text(&elem);
                // Format: "author / word_count / ..."
                let parts: Vec<&str> = text.split('/').collect();
                if parts.len() >= 2 {
                    info.author = parts[0].trim().to_string();
                    info.word_count = parts[1].trim().to_string();
                }
            }
            // Last text node in book_info3 might be update time
            let spans: Vec<scraper::ElementRef> = info_doc.select(&sel).collect();
            if let Some(last_span) = spans.last() {
                // Try to find additional text nodes for update time
                let all_text: Vec<&str> = last_span.text().collect();
                if all_text.len() >= 2 {
                    info.update_time = all_text.last().unwrap_or(&"").trim().to_string();
                }
            }
        }

        // Serial status from book_info2
        if let Ok(sel) = Selector::parse("div.book_info2 span") {
            let spans: Vec<scraper::ElementRef> = info_doc.select(&sel).collect();
            if spans.len() >= 2 {
                info.serial_status = element_text(&spans[1]);
            }
        }

        // Cover
        let cover_path = select_attr(&info_doc, "ul.book_info img", "src");
        if !cover_path.is_empty() {
            info.cover_url = if cover_path.starts_with("//") {
                format!("https:{}", cover_path)
            } else {
                cover_path
            };
        }

        // Summary
        info.summary = select_text(&info_doc, "ul.book_profile li.book_bk_qs1");

        // Parse catalog: div.mulu + following ul.mulu_list
        if let Ok(vol_sel) = Selector::parse("div.mulu") {
            for vol_div in catalog_doc.select(&vol_sel) {
                let vol_name = element_text(&vol_div).trim().to_string();

                let mut chapters = Vec::new();

                // The chapter list follows the div.mulu as a sibling
                // We need to find the next ul.mulu_list after this div
                // Since scraper doesn't have next_sibling easily, parse from full doc
                if let Ok(a_sel) = Selector::parse("ul.mulu_list a[href]") {
                    for a_elem in catalog_doc.select(&a_sel) {
                        let href = a_elem.value().attr("href").unwrap_or("").to_string();
                        if href.is_empty() {
                            continue;
                        }
                        // /c/{chapter_id}/ -> chapter_id
                        let chapter_id = href
                            .trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        let title = element_text(&a_elem).trim().to_string();
                        if title.is_empty() || chapter_id.is_empty() {
                            continue;
                        }
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id: chapter_id.clone(),
                            url: format!("{}/c/{}/", BASE, chapter_id),
                        });
                    }
                }

                if !chapters.is_empty() {
                    info.volumes.push(Volume {
                        volume_name: vol_name,
                        chapters,
                    });
                    // Only add chapters once (they were collected from the whole doc)
                    break;
                }
            }
        }

        // Fallback: if no volumes parsed from div.mulu, try a flat approach
        if info.volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(a_sel) = Selector::parse("ul.mulu_list a[href]") {
                for a_elem in catalog_doc.select(&a_sel) {
                    let href = a_elem.value().attr("href").unwrap_or("").to_string();
                    let chapter_id = href
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let title = element_text(&a_elem).trim().to_string();
                    if title.is_empty() || chapter_id.is_empty() {
                        continue;
                    }
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id: chapter_id.clone(),
                        url: format!("{}/c/{}/", BASE, chapter_id),
                    });
                }
            }
            if !chapters.is_empty() {
                info.volumes.push(Volume {
                    volume_name: String::new(),
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
        let url = format!("{}/c/{}/", BASE, chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        // Detect VIP image-based chapters
        if html_str.contains("/ajax/ashx/common.ashx") {
            let title = select_text(&doc, r#"ul.menu_top_list.book_view_top li:nth-child(2)"#);
            return Ok(Chapter {
                id: chapter_id.to_string(),
                title,
                content: "[VIP章节，需要订阅]".to_string(),
            });
        }

        // Check for locked VIP chapters
        if html_str.contains("本章为VIP章节") {
            let title = select_text(&doc, r#"ul.menu_top_list.book_view_top li:nth-child(2)"#);
            return Ok(Chapter {
                id: chapter_id.to_string(),
                title,
                content: "[VIP章节，需要订阅]".to_string(),
            });
        }

        // Title from top menu
        let title = select_text(&doc, r#"ul.menu_top_list.book_view_top li:nth-child(2)"#);

        // Content from div.yuedu.Content_Frame > div
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.yuedu.Content_Frame div") {
            if let Some(content_div) = doc.select(&sel).next() {
                let inner = content_div.inner_html();
                let text = html_to_text(&inner);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        paragraphs.push(trimmed.to_string());
                    }
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
    Box::new(SfacgProvider)
}
