use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct AlphapolisProvider;

impl AlphapolisProvider {
    fn normalize_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for AlphapolisProvider {
    fn name(&self) -> &str {
        "alphapolis"
    }

    fn display_name(&self) -> &str {
        "アルファポリス"
    }

    fn base_url(&self) -> &str {
        "https://www.alphapolis.co.jp"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let normalized = Self::normalize_book_id(book_id);
        let url = format!("{}/novel/{}", self.base_url(), normalized);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Title from h1 with class containing "title"
        if let Ok(sel) = Selector::parse("h1.title, h1[class*='title']") {
            if let Some(elem) = doc.select(&sel).next() {
                info.book_name = element_text(&elem);
            }
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author
        if let Ok(sel) = Selector::parse("div.author a, div[class*='author'] a") {
            if let Some(elem) = doc.select(&sel).next() {
                info.author = element_text(&elem);
            }
        }

        // Cover from og:image
        info.cover_url = meta_content(&doc, "og:image");

        // Summary
        if let Ok(sel) = Selector::parse("div.abstract") {
            if let Some(elem) = doc.select(&sel).next() {
                info.summary = element_text(&elem);
            }
        }
        if info.summary.is_empty() {
            info.summary = meta_content(&doc, "description");
        }

        // Update time from detail table
        if let Ok(sel) = Selector::parse("table.detail tr, table[class*='detail'] tr") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("更新日時") {
                    if let Ok(td_sel) = Selector::parse("td") {
                        if let Some(td) = elem.select(&td_sel).next() {
                            info.update_time = element_text(&td);
                        }
                    }
                }
            }
        }

        // Parse chapters with volume grouping from div.episodes
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        if let Ok(sel) = Selector::parse("div.episodes > *") {
            for elem in doc.select(&sel) {
                let tag = elem.value().name();
                if tag == "h3" {
                    // New volume
                    if !current_chapters.is_empty() {
                        volumes.push(Volume {
                            volume_name: current_volume_name.clone(),
                            chapters: std::mem::take(&mut current_chapters),
                        });
                    }
                    current_volume_name = element_text(&elem);
                } else if tag == "div" {
                    // Episode entry
                    if let Ok(a_sel) = Selector::parse("a[href]") {
                        if let Some(a_elem) = elem.select(&a_sel).next() {
                            if let Some(href) = a_elem.value().attr("href") {
                                // Title from span.title or a text
                                let title = if let Ok(title_sel) = Selector::parse("span.title") {
                                    if let Some(t) = elem.select(&title_sel).next() {
                                        element_text(&t)
                                    } else {
                                        element_text(&a_elem)
                                    }
                                } else {
                                    element_text(&a_elem)
                                };

                                if title.is_empty() {
                                    continue;
                                }

                                let chapter_id = href.rsplit('/').next().unwrap_or("").to_string();

                                current_chapters.push(ChapterInfo {
                                    title,
                                    chapter_id,
                                    url: normalize_url(self.base_url(), href),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Flush remaining chapters
        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_volume_name,
                chapters: current_chapters,
            });
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
        let normalized = Self::normalize_book_id(book_id);
        let url = format!(
            "{}/novel/{}/episode/{}",
            self.base_url(),
            normalized,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "h2.episode-title, h2[class*='episode-title']");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#novelBody") {
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
    Box::new(AlphapolisProvider)
}
