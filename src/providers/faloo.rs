use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct FalooProvider;

#[async_trait]
impl Provider for FalooProvider {
    fn name(&self) -> &str {
        "faloo"
    }

    fn display_name(&self) -> &str {
        "飞卢小说"
    }

    fn base_url(&self) -> &str {
        "https://b.faloo.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}.html", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name from meta or h1
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "#novelName");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author
        info.author = meta_content(&doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&doc, "a.colorQianHui, a[class*='colorQianHui']");
        }

        // Cover
        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&doc, "div.T-L-T-Img img", "src");
        }
        if info.cover_url.starts_with("//") {
            info.cover_url = format!("https:{}", info.cover_url);
        }

        // Status and update
        info.serial_status = meta_content(&doc, "og:novel:status");
        info.update_time = meta_content(&doc, "og:novel:update_time");

        // Summary
        info.summary = select_text(&doc, "div.T-L-T-C-Box1 p");
        if info.summary.is_empty() {
            info.summary = meta_content(&doc, "og:description");
        }

        // Parse chapters from DivTable links
        let mut volumes: Vec<Volume> = Vec::new();

        // Try volume-based structure
        let volume_selectors = [
            ("作品相关", "div.C-L-T-C-Box1"),
            ("正文", "div#mulu, div.C-L-T-C-Box2"),
            ("VIP正文", "div.C-L-T-C-Box3"),
        ];

        for (vol_name, vol_sel_str) in &volume_selectors {
            if let Ok(vol_sel) = Selector::parse(vol_sel_str) {
                for vol_elem in doc.select(&vol_sel) {
                    let mut chapters = Vec::new();
                    if let Ok(a_sel) = Selector::parse("div.DivTable a[href], div[class*='DivTable'] a[href], a[href]") {
                        for a_elem in vol_elem.select(&a_sel) {
                            if let Some(href) = a_elem.value().attr("href") {
                                let title = element_text(&a_elem);
                                if title.is_empty() {
                                    continue;
                                }
                                // Extract chapter ID: /{book_id}_{chapter_id}.html
                                let chapter_id = href
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or("")
                                    .split('.')
                                    .next()
                                    .unwrap_or("")
                                    .to_string();
                                // Extract just the chapter part after underscore
                                let chapter_id = if let Some(pos) = chapter_id.rfind('_') {
                                    chapter_id[pos + 1..].to_string()
                                } else {
                                    chapter_id
                                };
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
                            volume_name: vol_name.to_string(),
                            chapters,
                        });
                    }
                }
            }
        }

        // Fallback: collect all chapter links
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("a[href]") {
                for elem in doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        if !href.contains(&format!("{}_", book_id)) {
                            continue;
                        }
                        let title = element_text(&elem);
                        if title.is_empty() {
                            continue;
                        }
                        let chapter_id = href
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        let chapter_id = if let Some(pos) = chapter_id.rfind('_') {
                            chapter_id[pos + 1..].to_string()
                        } else {
                            chapter_id
                        };
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
                    volume_name: "正文".to_string(),
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
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "{}/{}_{}.html",
            self.base_url(),
            book_id,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "div.c_l_title h1");
        if title.is_empty() {
            let _ = select_text(&doc, "h1");
        }

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("div.noveContent") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(FalooProvider)
}
