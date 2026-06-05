use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct HaiwaishubaoProvider;

#[async_trait]
impl Provider for HaiwaishubaoProvider {
    fn name(&self) -> &str {
        "haiwaishubao"
    }

    fn display_name(&self) -> &str {
        "海外书包"
    }

    fn base_url(&self) -> &str {
        "https://www.haiwaishubao.com"
    }

    fn supports_search(&self) -> bool {
        true
    }

    async fn search(
        &self,
        client: &HttpClient,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let html_str = client
            .post_form(
                "https://www.haiwaishubao.com/search/",
                &[
                    ("searchkey", keyword),
                    ("searchtype", "all"),
                    ("submit", ""),
                ],
            )
            .await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.SHsectionThree-middle p") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "a[href*='/book/']", "href");
                if href.is_empty() {
                    continue;
                }

                let book_id = href
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .to_string();

                let title = select_text_in(&elem, "a[href*='/book/']");
                let author = select_text_in(&elem, "a[href*='/author/']");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://www.haiwaishubao.com/book/{}/", book_id);
        let info_html = client.get(&info_url).await?;

        // Parse info_doc in its own scope so it's dropped before any await
        let mut info = {
            let info_doc = Html::parse_document(&info_html);
            let mut info = BookInfo::default();

            info.book_name = meta_content(&info_doc, "og:title");
            if info.book_name.is_empty() {
                info.book_name = select_text(&info_doc, "p.title");
            }

            info.author = meta_content(&info_doc, "og:novel:author");
            if info.author.is_empty() {
                info.author = select_text(&info_doc, "p.author a");
            }

            info.cover_url = meta_content(&info_doc, "og:image");
            info.update_time = meta_content(&info_doc, "og:novel:update_time");
            info.serial_status = meta_content(&info_doc, "og:novel:status");
            info.summary = meta_content(&info_doc, "og:description").replace("&emsp;", "");
            info
        };

        // Fetch paginated catalog pages
        let mut all_chapters = Vec::new();
        let mut page = 1;
        loop {
            let catalog_url = if page == 1 {
                format!("https://www.haiwaishubao.com/index/{}/", book_id)
            } else {
                format!("https://www.haiwaishubao.com/index/{}/{}/", book_id, page)
            };

            let catalog_html = match client.get(&catalog_url).await {
                Ok(h) => h,
                Err(_) => break,
            };

            let (page_chapters, has_next) = {
                let catalog_doc = Html::parse_document(&catalog_html);
                let mut chapters = Vec::new();
                if let Ok(sel) = Selector::parse("ol.BCsectionTwo-top a[href]") {
                    for elem in catalog_doc.select(&sel) {
                        if let Some(href) = elem.value().attr("href") {
                            let title = element_text(&elem);
                            if title.is_empty() {
                                continue;
                            }
                            let chapter_id = href
                                .rsplit('/')
                                .next()
                                .unwrap_or(href)
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

                let next_page_path = format!("/index/{}/{}/", book_id, page + 1);
                let has_next = catalog_html.contains(&next_page_path);
                (chapters, has_next)
            };

            if page_chapters.is_empty() {
                break;
            }
            all_chapters.extend(page_chapters);

            if !has_next {
                break;
            }
            page += 1;
            if page > 50 {
                break;
            }
        }

        if !all_chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: "正文".to_string(),
                chapters: all_chapters,
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
        let mut title = String::new();
        let mut all_paragraphs = Vec::new();

        // Handle paginated chapters
        let mut page = 1;
        loop {
            let url = if page == 1 {
                format!(
                    "https://www.haiwaishubao.com/book/{}/{}.html",
                    book_id, chapter_id
                )
            } else {
                format!(
                    "https://www.haiwaishubao.com/book/{}/{}_{}.html",
                    book_id, chapter_id, page
                )
            };

            let html_str = match client.get(&url).await {
                Ok(h) => h,
                Err(_) => break,
            };

            let (page_title, paragraphs, has_next) = {
                let doc = Html::parse_document(&html_str);

                let page_title = select_text(&doc, "#chapterTitle");

                let mut paragraphs = Vec::new();
                if let Ok(sel) = Selector::parse("#content p") {
                    paragraphs = doc
                        .select(&sel)
                        .map(|p| element_text(&p))
                        .filter(|t| !t.is_empty())
                        .collect();
                }

                let next_page_url = format!("/book/{}/{}_{}.html", book_id, chapter_id, page + 1);
                let has_next = html_str.contains(&next_page_url);
                (page_title, paragraphs, has_next)
            };

            if title.is_empty() {
                title = page_title;
            }

            if paragraphs.is_empty() && page > 1 {
                break;
            }
            all_paragraphs.extend(paragraphs);

            if !has_next {
                break;
            }
            page += 1;
            if page > 20 {
                break;
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: all_paragraphs.join("\n"),
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(HaiwaishubaoProvider)
}
