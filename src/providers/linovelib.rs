use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 哔哩轻小说 (www.linovelib.com) provider.
///
/// Supports paginated chapter reading. Character substitution and
/// paragraph shuffling are skipped for simplicity.
pub struct LinovelibProvider;

const BASE: &str = "https://www.linovelib.com";

#[async_trait]
impl Provider for LinovelibProvider {
    fn name(&self) -> &str {
        "linovelib"
    }

    fn display_name(&self) -> &str {
        "哔哩轻小说"
    }

    fn base_url(&self) -> &str {
        BASE
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/novel/{}.html", BASE, book_id);
        let info_html = client.get(&info_url).await?;

        // Parse info in a block so Html is dropped before any await
        let (mut info, mut vol_ids) = {
            let info_doc = Html::parse_document(&info_html);
            let mut info = BookInfo::default();

            info.book_name = meta_content(&info_doc, "og:novel:book_name");
            if info.book_name.is_empty() {
                info.book_name = meta_content(&info_doc, "og:title");
            }
            if info.book_name.is_empty() {
                info.book_name = select_text(&info_doc, "h1.book-name");
            }

            info.author = meta_content(&info_doc, "og:novel:author");
            if info.author.is_empty() {
                info.author = meta_content(&info_doc, "author");
            }
            if info.author.is_empty() {
                info.author = select_text(&info_doc, "div.book-author div.au-name a");
            }

            info.cover_url = meta_content(&info_doc, "og:image");
            if info.cover_url.is_empty() {
                info.cover_url = meta_content(&info_doc, "pic");
            }

            info.serial_status = meta_content(&info_doc, "og:novel:status");
            if info.serial_status.is_empty() {
                info.serial_status = select_text(&info_doc, "div.book-label a.state");
            }

            info.summary = meta_content(&info_doc, "og:description");
            if info.summary.is_empty() {
                info.summary = select_text(&info_doc, "div.book-dec p");
            }

            let wc = select_text(&info_doc, "div.nums span");
            if wc.contains("字数") {
                info.word_count = wc.replace("字数：", "").trim().to_string();
            }

            info.update_time = meta_content(&info_doc, "og:novel:update_time");

            let vol_id_re =
                Regex::new(&format!(r"/novel/{}/([^.]+)\.html", regex::escape(book_id)))
                    .unwrap_or_else(|_| Regex::new(r"/novel/\d+/(vol_\d+)\.html").unwrap());

            let mut vol_ids: Vec<String> = vol_id_re
                .captures_iter(&info_html)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .filter(|id| id.starts_with("vol_"))
                .collect();
            vol_ids.dedup();
            vol_ids.reverse();

            (info, vol_ids)
        }; // info_doc dropped here

        // If no volumes found in info page, try catalog page
        if vol_ids.is_empty() {
            let catalog_url = format!("{}/novel/{}/catalog", BASE, book_id);
            if let Ok(catalog_html) = client.get(&catalog_url).await {
                let vol_id_re =
                    Regex::new(&format!(r"/novel/{}/([^.]+)\.html", regex::escape(book_id)))
                        .unwrap_or_else(|_| Regex::new(r"/novel/\d+/(vol_\d+)\.html").unwrap());
                let mut found: Vec<String> = vol_id_re
                    .captures_iter(&catalog_html)
                    .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                    .filter(|id| id.starts_with("vol_"))
                    .collect();
                found.dedup();
                vol_ids = found;
            }
        }

        // Fetch each volume page and extract chapters
        for vol_id in &vol_ids {
            let vol_url = format!("{}/novel/{}/{}.html", BASE, book_id, vol_id);
            let vol_html = match client.get(&vol_url).await {
                Ok(h) => h,
                Err(_) => continue,
            };
            let vol_doc = Html::parse_document(&vol_html);

            // Volume name = og:title minus the book name prefix
            let mut vol_full_title = meta_content(&vol_doc, "og:title");
            if vol_full_title.is_empty() {
                vol_full_title = select_text(&vol_doc, "h1.book-name");
            }
            let volume_name =
                if !info.book_name.is_empty() && vol_full_title.starts_with(&info.book_name) {
                    vol_full_title[info.book_name.len()..]
                        .trim_start_matches(|c: char| " ：:·-—".contains(c))
                        .to_string()
                } else {
                    vol_full_title
                };

            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("div.book-new-chapter a[href]") {
                for a_elem in vol_doc.select(&sel) {
                    let title = element_text(&a_elem);
                    let href = a_elem.value().attr("href").unwrap_or("").to_string();
                    // /novel/{book_id}/{chapter_id}.html -> chapter_id
                    let chapter_id = href
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if title.is_empty() || chapter_id.is_empty() {
                        continue;
                    }
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id: chapter_id.clone(),
                        url: format!("{}/novel/{}/{}.html", BASE, book_id, chapter_id),
                    });
                }
            }

            if !chapters.is_empty() {
                info.volumes.push(Volume {
                    volume_name,
                    chapters,
                });
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
        let mut all_paragraphs = Vec::new();
        let mut title = String::new();
        let mut page_idx = 1u32;

        loop {
            let suffix = if page_idx == 1 {
                format!("/novel/{}/{}.html", book_id, chapter_id)
            } else {
                format!("/novel/{}/{}_{}.html", book_id, chapter_id, page_idx)
            };
            let url = format!("{}{}", BASE, suffix);

            let html_str = match client.get(&url).await {
                Ok(h) => h,
                Err(_) => break,
            };

            let doc = Html::parse_document(&html_str);

            // Extract title from first page
            if title.is_empty() {
                title = select_text(&doc, "div#mlfy_main_text h1");
                if title.is_empty() {
                    title = select_text(&doc, "h1");
                }
            }

            // Extract content from #TextContent
            if let Ok(sel) = Selector::parse("#TextContent") {
                if let Some(tc) = doc.select(&sel).next() {
                    // Get text from <p> elements
                    if let Ok(p_sel) = Selector::parse("p") {
                        for p in tc.select(&p_sel) {
                            let text = element_text(&p).trim().to_string();
                            if !text.is_empty() {
                                all_paragraphs.push(text);
                            }
                        }
                    }
                }
            }

            // Check if next page exists
            page_idx += 1;
            let next_suffix = format!("/novel/{}/{}_{}.html", book_id, chapter_id, page_idx);
            if !html_str.contains(&next_suffix) {
                break;
            }
        }

        let content = all_paragraphs.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(LinovelibProvider)
}
