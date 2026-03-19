use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// Syosetu18 is identical to Syosetu but uses novel18.syosetu.com
/// and requires an over18=yes cookie.
pub struct Syosetu18Provider;

impl Syosetu18Provider {
    fn info_url(&self, book_id: &str, page: usize) -> String {
        if page > 1 {
            format!("https://novel18.syosetu.com/{}/?p={}", book_id, page)
        } else {
            format!("https://novel18.syosetu.com/{}/", book_id)
        }
    }

    fn parse_all_volumes(all_pages: &[String], base_url: &str) -> Vec<Volume> {
        let mut volumes: Vec<Volume> = Vec::new();
        let mut vol_idx = 1;
        let mut current_vol_name: Option<String> = None;
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        for page_html in all_pages {
            let page_doc = Html::parse_document(page_html);
            if let Ok(sel) = Selector::parse("div.p-eplist > *") {
                for elem in page_doc.select(&sel) {
                    let classes = elem.value().attr("class").unwrap_or("");
                    if classes.contains("p-eplist__chapter-title") {
                        if !current_chapters.is_empty() {
                            volumes.push(Volume {
                                volume_name: current_vol_name
                                    .take()
                                    .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                                chapters: std::mem::take(&mut current_chapters),
                            });
                            vol_idx += 1;
                        }
                        current_vol_name = Some(element_text(&elem));
                    } else if classes.contains("p-eplist__sublist") {
                        if let Ok(a_sel) = Selector::parse("a.p-eplist__subtitle") {
                            if let Some(a) = elem.select(&a_sel).next() {
                                let href = a.value().attr("href").unwrap_or("").trim();
                                let title = element_text(&a);
                                if !href.is_empty() && !title.is_empty() {
                                    let chap_id = href
                                        .trim_matches('/')
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or("")
                                        .to_string();
                                    current_chapters.push(ChapterInfo {
                                        title,
                                        chapter_id: chap_id,
                                        url: normalize_url(base_url, href),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_vol_name
                    .take()
                    .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                chapters: current_chapters,
            });
        }

        volumes
    }
}

#[async_trait]
impl Provider for Syosetu18Provider {
    fn name(&self) -> &str {
        "syosetu18"
    }

    fn display_name(&self) -> &str {
        "小説家になろう 18禁"
    }

    fn base_url(&self) -> &str {
        "https://novel18.syosetu.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let first_url = self.info_url(book_id, 1);
        let mut headers = HeaderMap::new();
        headers.insert("Cookie", HeaderValue::from_static("over18=yes"));
        let first_html = client.get_with_headers(&first_url, headers.clone()).await?;

        // Extract metadata in a non-async block to avoid Send issues with Html
        let (mut info, has_more_pages) = {
            let doc = Html::parse_document(&first_html);
            let mut info = BookInfo::default();
            info.book_name = select_text(&doc, "h1.p-novel__title");
            info.author = select_text(&doc, "div.p-novel__author a");
            if info.author.is_empty() {
                info.author = select_text(&doc, "div.p-novel__author")
                    .replace("作者：", "");
            }
            info.cover_url = meta_content(&doc, "og:image");
            if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
                info.cover_url = format!("https:{}", info.cover_url);
            }
            info.summary = select_text(&doc, "#novel_ex");
            let has_more = first_html.contains("/?p=2");
            (info, has_more)
        };

        // Collect paginated pages
        let mut all_pages = vec![first_html];
        if has_more_pages {
            let mut page = 2;
            loop {
                let next_marker = format!("/?p={}", page);
                let prev_html = &all_pages[all_pages.len() - 1];
                if !prev_html.contains(&next_marker) {
                    break;
                }
                if let Ok(page_html) = client
                    .get_with_headers(&self.info_url(book_id, page), headers.clone())
                    .await
                {
                    all_pages.push(page_html);
                }
                page += 1;
                if page > 100 {
                    break;
                }
            }
        }

        // Parse volumes and chapters (non-async)
        let base = self.base_url().to_string();
        info.volumes = Self::parse_all_volumes(&all_pages, &base);
        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "https://novel18.syosetu.com/{}/{}/",
            book_id, chapter_id
        );
        let mut headers = HeaderMap::new();
        headers.insert("Cookie", HeaderValue::from_static("over18=yes"));
        let html_text = client.get_with_headers(&url, headers).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "h1.p-novel__title");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.p-novel__body div.p-novel__text p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                if !text.is_empty() {
                    paragraphs.push(text);
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
    Box::new(Syosetu18Provider)
}
