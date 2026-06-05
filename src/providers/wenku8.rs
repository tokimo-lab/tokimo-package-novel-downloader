use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 轻小说文库 (www.wenku8.net) provider.
///
/// Uses GBK encoding. Catalog pages use a table-based layout with
/// volume rows (td.vcss) and chapter rows (td.ccss).
pub struct Wenku8Provider;

const BASE: &str = "https://www.wenku8.net";

/// Compute the directory prefix for wenku8 URLs.
/// IDs with 3 or fewer digits are placed in directory "0",
/// otherwise prefix = book_id without the last 3 digits.
fn compute_prefix(book_id: &str) -> String {
    if book_id.len() <= 3 {
        "0".to_string()
    } else {
        book_id[..book_id.len() - 3].to_string()
    }
}

#[async_trait]
impl Provider for Wenku8Provider {
    fn name(&self) -> &str {
        "wenku8"
    }

    fn display_name(&self) -> &str {
        "轻小说文库"
    }

    fn base_url(&self) -> &str {
        BASE
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let prefix = compute_prefix(book_id);

        let info_url = format!("{}/book/{}.htm", BASE, book_id);
        let catalog_url = format!("{}/novel/{}/{}/index.htm", BASE, prefix, book_id);

        // Wenku8 uses GBK encoding
        let info_html = client
            .get_with_encoding(&info_url, encoding_rs::GBK)
            .await?;
        let catalog_html = client
            .get_with_encoding(&catalog_url, encoding_rs::GBK)
            .await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        // Book name from <b> in table
        info.book_name = select_text(&info_doc, "table b");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "h1");
        }

        // Author – from td containing "小说作者"
        if let Ok(sel) = Selector::parse("td") {
            for elem in info_doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("小说作者") {
                    info.author = text.replace("小说作者：", "").trim().to_string();
                    break;
                }
            }
        }

        // Cover
        info.cover_url = select_attr(&info_doc, r#"img[src*="/image/"]"#, "src");

        // Serial status
        if let Ok(sel) = Selector::parse("td") {
            for elem in info_doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("文章状态") {
                    info.serial_status = text.replace("文章状态：", "").trim().to_string();
                    break;
                }
            }
        }

        // Word count
        if let Ok(sel) = Selector::parse("td") {
            for elem in info_doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("全文长度") {
                    info.word_count = text.replace("全文长度：", "").trim().to_string();
                    break;
                }
            }
        }

        // Update time
        if let Ok(sel) = Selector::parse("td") {
            for elem in info_doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("最后更新") {
                    info.update_time = text.replace("最后更新：", "").trim().to_string();
                    break;
                }
            }
        }

        // Summary – span after "内容简介"
        if let Ok(sel) = Selector::parse("span") {
            let mut found_intro = false;
            for elem in info_doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("内容简介") {
                    found_intro = true;
                    continue;
                }
                if found_intro {
                    info.summary = text;
                    break;
                }
            }
        }

        // Parse catalog: table.css rows
        // Volume headers: td.vcss, Chapter links: td.ccss a
        let mut current_vol_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();
        let mut vol_idx = 1;

        if let Ok(tr_sel) = Selector::parse("table.css tr") {
            for tr in catalog_doc.select(&tr_sel) {
                // Check for volume header
                if let Ok(vcss_sel) = Selector::parse("td.vcss") {
                    if let Some(vcss) = tr.select(&vcss_sel).next() {
                        // Flush previous volume
                        if !current_chapters.is_empty() {
                            let vn = if current_vol_name.is_empty() {
                                format!("未命名卷 {}", vol_idx)
                            } else {
                                current_vol_name.clone()
                            };
                            info.volumes.push(Volume {
                                volume_name: vn,
                                chapters: std::mem::take(&mut current_chapters),
                            });
                            vol_idx += 1;
                        }
                        current_vol_name = element_text(&vcss).trim().to_string();
                        continue;
                    }
                }

                // Extract chapter links from td.ccss
                if let Ok(ccss_sel) = Selector::parse("td.ccss a[href]") {
                    for a_elem in tr.select(&ccss_sel) {
                        let title = element_text(&a_elem);
                        let href = a_elem.value().attr("href").unwrap_or("").to_string();
                        if title.is_empty() || href.is_empty() {
                            continue;
                        }
                        // href like "12345.htm" -> chapter_id "12345"
                        let chapter_id = href.split('.').next().unwrap_or("").to_string();
                        current_chapters.push(ChapterInfo {
                            title,
                            chapter_id: chapter_id.clone(),
                            url: format!(
                                "{}/novel/{}/{}/{}.htm",
                                BASE, prefix, book_id, chapter_id
                            ),
                        });
                    }
                }
            }
        }

        // Flush last volume
        if !current_chapters.is_empty() {
            let vn = if current_vol_name.is_empty() {
                format!("未命名卷 {}", vol_idx)
            } else {
                current_vol_name
            };
            info.volumes.push(Volume {
                volume_name: vn,
                chapters: current_chapters,
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
        let prefix = compute_prefix(book_id);
        let url = format!("{}/novel/{}/{}/{}.htm", BASE, prefix, book_id, chapter_id);
        let html_str = client.get_with_encoding(&url, encoding_rs::GBK).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, r#"div#title"#);

        let mut paragraphs = Vec::new();

        // Parse content div – iterate children
        if let Ok(sel) = Selector::parse("div#content") {
            if let Some(content_elem) = doc.select(&sel).next() {
                let inner = content_elem.inner_html();
                // Split on <br> tags and extract text
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
    Box::new(Wenku8Provider)
}
