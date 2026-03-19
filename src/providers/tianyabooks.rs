use anyhow::Result;
use async_trait::async_trait;
use encoding_rs::GBK;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 天涯书库 provider (GBK encoded)
pub struct TianyabooksProvider;

impl TianyabooksProvider {
    fn transform_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for TianyabooksProvider {
    fn name(&self) -> &str {
        "tianyabooks"
    }

    fn display_name(&self) -> &str {
        "天涯书库"
    }

    fn base_url(&self) -> &str {
        "https://www.tianyabooks.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let real_id = Self::transform_book_id(book_id);
        let url = format!("https://www.tianyabooks.com/{}/", real_id);
        let html_text = client.get_with_encoding(&url, GBK).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        // Book name with fallbacks
        info.book_name = select_text(&doc, "div.catalog h1");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "div.book h1");
        }
        if info.book_name.is_empty() {
            info.book_name = "未知书名".to_string();
        }

        // Author with fallbacks
        info.author = select_text(&doc, "div.catalog h2 a");
        if info.author.is_empty() {
            info.author = select_text(&doc, "div.book h2 a");
        }
        if info.author.is_empty() {
            info.author = select_text(&doc, "div.catalog div.info, div.book h2")
                .replace("作者：", "");
        }
        if info.author.is_empty() {
            info.author = "未知作者".to_string();
        }

        // Summary
        info.summary = select_text(&doc, "div.description p");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "div.summary p");
        }
        if info.summary.is_empty() {
            info.summary = "无简介".to_string();
        }

        // Parse volumes with multiple XPath-equivalent strategies
        let volume_strategies: Vec<&str> = vec![
            "dl > *",
            "div.mulu-title, div.mulu-list",
            "div.idx-title, div.idx-list",
        ];

        for strategy in &volume_strategies {
            let volumes = self.parse_volume_nodes(&doc, strategy);
            if !volumes.is_empty() {
                info.volumes = volumes;
                break;
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
        let real_id = Self::transform_book_id(book_id);
        let url = format!(
            "https://www.tianyabooks.com/{}/{}.html",
            real_id, chapter_id
        );
        let html_text = client.get_with_encoding(&url, GBK).await?;
        let doc = Html::parse_document(&html_text);

        // Title with fallbacks
        let title_selectors = [
            "div.article h2",
            "div#main h1",
            "div.book h1",
            "div.content h1",
        ];
        let mut title = String::new();
        for sel_str in &title_selectors {
            title = select_text(&doc, sel_str);
            if !title.is_empty() {
                break;
            }
        }

        // Content with fallbacks
        let content_selectors = [
            "div.article p",
            "div#main p",
            "div#neirong p",
            "div.book p",
            "div.content p",
        ];
        let mut paragraphs = Vec::new();
        for sel_str in &content_selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                for p in doc.select(&sel) {
                    let text = element_text(&p);
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                }
                if !paragraphs.is_empty() {
                    break;
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

impl TianyabooksProvider {
    fn parse_volume_nodes(&self, doc: &Html, selector_str: &str) -> Vec<Volume> {
        let mut volumes = Vec::new();
        let mut vol_idx = 1;
        let mut vol_name: Option<String> = None;
        let mut vol_chaps: Vec<ChapterInfo> = Vec::new();

        let sel = match Selector::parse(selector_str) {
            Ok(s) => s,
            Err(_) => return volumes,
        };

        for elem in doc.select(&sel) {
            let tag = elem.value().name();
            let cls = elem.value().attr("class").unwrap_or("");

            // Title node (dt or *-title class)
            if tag == "dt" || cls.contains("title") {
                if !vol_chaps.is_empty() {
                    volumes.push(Volume {
                        volume_name: vol_name
                            .take()
                            .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                        chapters: std::mem::take(&mut vol_chaps),
                    });
                    vol_idx += 1;
                }
                vol_name = Some(element_text(&elem));
            }
            // List node (dd or *-list class)
            else if tag == "dd" || cls.contains("list") {
                if let Ok(a_sel) = Selector::parse("a[href]") {
                    for a in elem.select(&a_sel) {
                        let href = a.value().attr("href").unwrap_or("").trim();
                        if href.is_empty() {
                            continue;
                        }
                        let title = element_text(&a);
                        let chap_id = href
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        vol_chaps.push(ChapterInfo {
                            title,
                            chapter_id: chap_id,
                            url: normalize_url(self.base_url(), href),
                        });
                    }
                }
            }
        }

        // Flush last volume
        if !vol_chaps.is_empty() {
            volumes.push(Volume {
                volume_name: vol_name
                    .take()
                    .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                chapters: vol_chaps,
            });
        }

        volumes
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(TianyabooksProvider)
}
