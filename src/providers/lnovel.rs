use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::select_text_in;

pub struct LnovelProvider;

#[async_trait]
impl Provider for LnovelProvider {
    fn name(&self) -> &str {
        "lnovel"
    }

    fn display_name(&self) -> &str {
        "轻小说百科"
    }

    fn base_url(&self) -> &str {
        "https://lnovel.org"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/books-{}", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Title
        info.book_name = select_text(&doc, "main h1");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Metadata from dl > dt + dd pairs
        if let Ok(dl_sel) = Selector::parse("dl") {
            for dl_elem in doc.select(&dl_sel) {
                let mut last_dt_text = String::new();
                for child in dl_elem.children() {
                    if let Some(el) = scraper::ElementRef::wrap(child) {
                        let tag = el.value().name();
                        if tag == "dt" {
                            last_dt_text = element_text(&el);
                        } else if tag == "dd" && !last_dt_text.is_empty() {
                            if last_dt_text.contains("作者") {
                                if let Ok(a_sel) = Selector::parse("a") {
                                    if let Some(a) = el.select(&a_sel).next() {
                                        info.author = element_text(&a);
                                    }
                                }
                                if info.author.is_empty() {
                                    info.author = element_text(&el);
                                }
                            } else if last_dt_text.contains("更新") {
                                info.update_time = element_text(&el);
                            } else if last_dt_text.contains("状态") {
                                info.serial_status = element_text(&el);
                            }
                            last_dt_text.clear();
                        }
                    }
                }
            }
        }

        // Cover from og:image
        info.cover_url = meta_content(&doc, "og:image");
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = normalize_url(self.base_url(), &info.cover_url);
        }

        // Summary
        info.summary = select_text(&doc, "p.my-2");
        if info.summary.is_empty() {
            info.summary = meta_content(&doc, "description");
        }

        // Parse volumes from accordion items in #volumes
        let mut volumes: Vec<Volume> = Vec::new();
        if let Ok(vol_sel) = Selector::parse("#volumes div.accordion-item, #volumes div[class*='accordion-item']") {
            for vol_elem in doc.select(&vol_sel) {
                let volume_name = select_text_in(&vol_elem, "button, .accordion-header, h2");

                let mut chapters = Vec::new();
                if let Ok(a_sel) = Selector::parse("a[href]") {
                    for a_elem in vol_elem.select(&a_sel) {
                        if let Some(href) = a_elem.value().attr("href") {
                            if !href.contains("chapters-") {
                                continue;
                            }
                            let title = element_text(&a_elem);
                            if title.is_empty() {
                                continue;
                            }
                            // Extract chapter ID from /chapters-{id}
                            let chapter_id = href
                                .rsplit("chapters-")
                                .next()
                                .unwrap_or("")
                                .split('?')
                                .next()
                                .unwrap_or("")
                                .split('#')
                                .next()
                                .unwrap_or("")
                                .to_string();
                            chapters.push(ChapterInfo {
                                title,
                                chapter_id,
                                url: normalize_url(self.base_url(), href),
                            });
                        }
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

        // Fallback: collect all chapter links
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("a[href*='chapters-']") {
                for elem in doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        let title = element_text(&elem);
                        if title.is_empty() {
                            continue;
                        }
                        let chapter_id = href
                            .rsplit("chapters-")
                            .next()
                            .unwrap_or("")
                            .split('?')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id,
                            url: normalize_url(self.base_url(), href),
                        });
                    }
                }
            }
            if !chapters.is_empty() {
                volumes.push(Volume {
                    volume_name: String::new(),
                    chapters,
                });
            }
        }

        info.volumes = volumes;
        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/chapters-{}", self.base_url(), chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut title = select_text(&doc, "main h1");
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        // Content from #chaptersShowContent p
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#chaptersShowContent p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        // Fallback to full div content
        if paragraphs.is_empty() {
            if let Ok(sel) = Selector::parse("#chaptersShowContent") {
                if let Some(elem) = doc.select(&sel).next() {
                    let content = html_to_text(&elem.inner_html());
                    return Ok(Chapter {
                        id: chapter_id.to_string(),
                        title,
                        content,
                    });
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
    Box::new(LnovelProvider)
}
