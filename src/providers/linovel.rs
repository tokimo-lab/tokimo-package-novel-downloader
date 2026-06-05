use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::select_text_in;
use crate::types::*;
use crate::utils::*;

pub struct LinovelProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(LinovelProvider)
}

const BASE_URL: &str = "https://www.linovel.net";

#[async_trait]
impl Provider for LinovelProvider {
    fn name(&self) -> &str {
        "linovel"
    }

    fn display_name(&self) -> &str {
        "轻之文库"
    }

    fn base_url(&self) -> &str {
        BASE_URL
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
        let url = format!("{}/search/?kw={}", BASE_URL, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("a.search-book[href]") {
            for elem in doc.select(&sel).take(limit) {
                let href = elem.value().attr("href").unwrap_or("");
                if href.is_empty() || !href.contains("book/") {
                    continue;
                }
                let book_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let title = select_text_in(&elem, "div.book-name");
                let author_extra = select_text_in(&elem, "div.book-extra");

                let (author, update_date) = if author_extra.contains('丨') {
                    let parts: Vec<&str> = author_extra.splitn(2, '丨').collect();
                    (
                        parts[0].trim().to_string(),
                        parts.get(1).unwrap_or(&"").trim().to_string(),
                    )
                } else {
                    (author_extra.trim().to_string(), String::new())
                };

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date,
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/book/{}.html", BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();

        // Core fields from og meta tags with fallbacks
        info.book_name = meta_content(&doc, "og:title");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1.book-title");
        }

        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.is_empty() {
            let v = select_attr(&doc, "div.book-cover img", "src");
            if v.is_empty() {
                info.cover_url = select_attr(&doc, "div.book-cover a", "href");
            } else {
                info.cover_url = v;
            }
        }

        info.author = select_text(&doc, "div.sidebar div.novelist div.name a");

        info.update_time = select_text(&doc, "div.book-last-update")
            .replace("更新于", "")
            .replace('\u{00a0}', " ")
            .trim()
            .to_string();

        info.summary =
            select_text(&doc, "div.section.introduction div.about-text").replace('\u{00a0}', " ");

        // Volumes & Chapters
        if let Ok(sec_sel) = Selector::parse("div.section-list div.section[data-index-name]") {
            for sec in doc.select(&sec_sel) {
                let volume_name = select_text_in(&sec, "h2.volume-title");

                let mut chapters = Vec::new();
                if let Ok(a_sel) = Selector::parse("div.chapter-list a[href]") {
                    for a in sec.select(&a_sel) {
                        let ch_url = a.value().attr("href").unwrap_or("").trim();
                        if ch_url.is_empty() || ch_url.starts_with("javascript:") {
                            continue;
                        }
                        let ch_title = element_text(&a);
                        let ch_id = ch_url
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or("")
                            .to_string();

                        chapters.push(ChapterInfo {
                            title: ch_title,
                            chapter_id: ch_id,
                            url: normalize_url(BASE_URL, ch_url),
                        });
                    }
                }

                info.volumes.push(Volume {
                    volume_name,
                    chapters,
                });
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
        let url = format!("{}/book/{}/{}.html", BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "div.article-title");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.article-text p.l") {
            for p in doc.select(&sel) {
                let classes = p.value().attr("class").unwrap_or("");
                if classes.contains("l-image") {
                    // Skip image paragraphs for text content
                    continue;
                }
                let text = element_text(&p).replace('\u{00a0}', " ");
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: paragraphs.join("\n"),
        })
    }
}
