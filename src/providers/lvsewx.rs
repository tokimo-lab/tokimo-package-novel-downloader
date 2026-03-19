use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct LvsewxProvider;

impl LvsewxProvider {
    fn chapter_prefix(bid: &str) -> String {
        if bid.len() <= 3 {
            "0".to_string()
        } else {
            bid[..bid.len() - 3].to_string()
        }
    }
}

#[async_trait]
impl Provider for LvsewxProvider {
    fn name(&self) -> &str {
        "lvsewx"
    }

    fn display_name(&self) -> &str {
        "绿色小说网"
    }

    fn base_url(&self) -> &str {
        "https://www.lvsewx.cc"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/ebook/{}.html", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name from meta or HTML
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "div.info h2");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author
        info.author = meta_content(&doc, "og:novel:author");
        if info.author.is_empty() {
            if let Ok(sel) = Selector::parse("div.small span") {
                for elem in doc.select(&sel) {
                    let text = element_text(&elem);
                    if text.contains("作者") {
                        info.author = text
                            .replace("作者：", "")
                            .replace("作者:", "")
                            .trim()
                            .to_string();
                        break;
                    }
                }
            }
        }

        // Cover
        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.starts_with("//") {
            info.cover_url = format!("https:{}", info.cover_url);
        } else if info.cover_url.starts_with("/") {
            info.cover_url = format!("{}{}", self.base_url(), info.cover_url);
        }

        // Update time and status from meta
        info.update_time = meta_content(&doc, "og:novel:update_time");
        info.serial_status = meta_content(&doc, "og:novel:status");

        // Summary
        info.summary = select_text(&doc, "div.intro");
        info.summary = info
            .summary
            .replace("简介：", "")
            .trim()
            .to_string();
        if let Some(pos) = info.summary.find("作者：") {
            info.summary = info.summary[..pos].trim().to_string();
        }

        // Parse chapters from div.listmain dl
        let mut chapters = Vec::new();
        let mut found_main = false;

        if let Ok(sel) = Selector::parse("div.listmain dl > *") {
            for elem in doc.select(&sel) {
                let tag = elem.value().name();
                if tag == "dt" {
                    let text = element_text(&elem);
                    if text.contains("正文卷") || text.contains("正文") {
                        found_main = true;
                    }
                } else if tag == "dd" {
                    if let Ok(a_sel) = Selector::parse("a[href]") {
                        if let Some(a_elem) = elem.select(&a_sel).next() {
                            if let Some(href) = a_elem.value().attr("href") {
                                let title = element_text(&a_elem);
                                if title.is_empty() {
                                    continue;
                                }
                                let chapter_id = href
                                    .trim_end_matches(".html")
                                    .rsplit('/')
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
                }
            }
        }

        // Fallback
        if chapters.is_empty() {
            if let Ok(sel) = Selector::parse("div#listmain dd a[href], div.listmain dd a[href]") {
                for elem in doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        let title = element_text(&elem);
                        if title.is_empty() {
                            continue;
                        }
                        let chapter_id = href
                            .trim_end_matches(".html")
                            .rsplit('/')
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
        }

        if !chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: String::new(),
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
        let prefix = Self::chapter_prefix(book_id);
        let url = format!(
            "{}/books/{}/{}/{}.html",
            self.base_url(),
            prefix,
            book_id,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut title = select_text(&doc, "div.content h1");
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content") {
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
    Box::new(LvsewxProvider)
}
