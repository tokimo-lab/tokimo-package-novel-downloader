use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::select_text_in;
use crate::types::*;
use crate::utils::*;

pub struct DushuProvider;

#[async_trait]
impl Provider for DushuProvider {
    fn name(&self) -> &str {
        "dushu"
    }

    fn display_name(&self) -> &str {
        "读书"
    }

    fn base_url(&self) -> &str {
        "https://www.dushu.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/showbook/{}/", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&doc, "div.book-title h1");

        // Author from table row containing "作"
        if let Ok(sel) = Selector::parse("div.book-details table tr") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("作") {
                    if let Ok(td_sel) = Selector::parse("td:nth-child(2)") {
                        if let Some(td) = elem.select(&td_sel).next() {
                            info.author = element_text(&td);
                        }
                    }
                    break;
                }
            }
        }

        // Cover
        info.cover_url = select_attr(&doc, "div.book-pic img", "src");
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = normalize_url(self.base_url(), &info.cover_url);
        }

        // Summary
        let summary = select_text(&doc, "div.txtsummary, div[class*='txtsummary']");
        info.summary = summary
            .replace(['\u{3000}', '\u{00a0}'], " ")
            .trim()
            .to_string();

        // Volumes and chapters
        let mut volumes: Vec<Volume> = Vec::new();

        if let Ok(vol_sel) = Selector::parse("div.book-summary div.book-chapter") {
            for vol_elem in doc.select(&vol_sel) {
                let volume_name = select_text_in(&vol_elem, "h3, .title");

                let mut chapters = Vec::new();
                if let Ok(a_sel) = Selector::parse("a[href]") {
                    for a_elem in vol_elem.select(&a_sel) {
                        if let Some(href) = a_elem.value().attr("href") {
                            let title = element_text(&a_elem);
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

        // Fallback: try table-based chapter list
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("table a[href]") {
                for elem in doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        if !href.contains("/showbook/") && !href.contains("/chapter/") {
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
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "{}/showbook/{}/{}.html",
            self.base_url(),
            book_id,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        // Title from centered large text or h1
        let mut title = select_text(&doc, "p.text-center.text-large");
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("div.content_txt") {
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
    Box::new(DushuProvider)
}
