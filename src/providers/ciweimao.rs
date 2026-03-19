use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::select_text_in;

/// 刺猬猫 (www.ciweimao.com) provider.
///
/// Fetches book info from the main page and chapter list via an AJAX endpoint.
/// Text chapter content requires session/access-key negotiation which is
/// simplified here — only the basic HTML page content is extracted.
/// Image (VIP) chapters are detected but not decoded.
pub struct CiweimaoProvider;

const BASE: &str = "https://www.ciweimao.com";

#[async_trait]
impl Provider for CiweimaoProvider {
    fn name(&self) -> &str {
        "ciweimao"
    }

    fn display_name(&self) -> &str {
        "刺猬猫"
    }

    fn base_url(&self) -> &str {
        BASE
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/book/{}", BASE, book_id);
        let info_html = client.get(&info_url).await?;

        // Parse info in a block so Html is dropped before next await
        let mut info = {
            let info_doc = Html::parse_document(&info_html);
            let mut info = BookInfo::default();

            info.book_name = meta_content(&info_doc, "og:novel:book_name");
            if info.book_name.is_empty() {
                info.book_name = select_text(&info_doc, "h1.title");
            }

            info.author = meta_content(&info_doc, "og:novel:author");
            if info.author.is_empty() {
                info.author = select_text(&info_doc, "h1.title span a");
            }

            info.cover_url = meta_content(&info_doc, "og:image");
            if info.cover_url.is_empty() {
                info.cover_url = select_attr(&info_doc, "div.cover img", "src");
            }

            info.update_time = select_text(&info_doc, "p.update-time")
                .replace("最后更新：", "")
                .trim()
                .to_string();

            info.word_count = select_text(&info_doc, "p.book-grade b:last-child");
            info.serial_status = select_text(&info_doc, "p.update-state");

            info.summary = meta_content(&info_doc, "og:description");
            if info.summary.is_empty() {
                info.summary = select_text(&info_doc, "div.book-desc");
            }
            info
        }; // info_doc dropped here

        // Fetch chapter list via AJAX POST
        let chapter_list_url =
            format!("{}/chapter/get_chapter_list_in_chapter_detail", BASE);
        let form_data = [
            ("book_id", book_id),
            ("chapter_id", "0"),
            ("orderby", "0"),
        ];

        let catalog_html = client
            .post_form(&chapter_list_url, &form_data)
            .await
            .unwrap_or_default();

        if !catalog_html.is_empty() {
            let catalog_doc = Html::parse_document(&catalog_html);

            // Parse volumes: div.book-chapter-box
            if let Ok(vol_sel) = Selector::parse("div.book-chapter-box") {
                for vol_elem in catalog_doc.select(&vol_sel) {
                    let vol_name = select_text_in(&vol_elem, "h4.sub-tit");

                    let mut chapters = Vec::new();
                    if let Ok(a_sel) =
                        Selector::parse("ul.book-chapter-list a[href]")
                    {
                        for a_elem in vol_elem.select(&a_sel) {
                            let href = a_elem
                                .value()
                                .attr("href")
                                .unwrap_or("")
                                .to_string();
                            if href.is_empty() {
                                continue;
                            }
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
                                url: format!("{}/chapter/{}", BASE, chapter_id),
                            });
                        }
                    }

                    if !chapters.is_empty() {
                        info.volumes.push(Volume {
                            volume_name: if vol_name.is_empty() {
                                "正文".to_string()
                            } else {
                                vol_name
                            },
                            chapters,
                        });
                    }
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
        let url = format!("{}/chapter/{}", BASE, chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        // Detect image (VIP) chapters
        if html_str.contains("J_ImgRead") {
            let title = select_text(&doc, "div#J_BookCnt div.read-hd h1.chapter");
            return Ok(Chapter {
                id: chapter_id.to_string(),
                title,
                content: "[VIP图片章节，需要登录并订阅]".to_string(),
            });
        }

        // Title
        let title = select_text(&doc, "div#J_BookCnt div.read-hd h1.chapter");

        // For text chapters, the actual content is fetched via separate AJAX calls
        // that require session negotiation (session_code + chapter_access_key + decryption).
        // We extract whatever is available in the initial HTML page.
        let mut paragraphs = Vec::new();

        // Try to find content in the page
        for sel_str in &[
            "div.read-bd div.chapter-entity",
            "div#J_BookRead",
            "div.chapter-content",
            "div.read-content",
        ] {
            if let Ok(sel) = Selector::parse(sel_str) {
                if let Some(elem) = doc.select(&sel).next() {
                    let inner = elem.inner_html();
                    let text = html_to_text(&inner);
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            paragraphs.push(trimmed.to_string());
                        }
                    }
                    if !paragraphs.is_empty() {
                        break;
                    }
                }
            }
        }

        // Extract author note if present
        let author_say = select_text(&doc, "p.author_say");

        let mut content = paragraphs.join("\n");
        if !author_say.is_empty() {
            content.push_str("\n\n作者说\n");
            content.push_str(&author_say);
        }

        if content.is_empty() {
            content = "[需要登录查看章节内容]".to_string();
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(CiweimaoProvider)
}
