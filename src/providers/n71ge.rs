use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::select_attr_in;

pub struct N71geProvider;

impl N71geProvider {
    const BASE_URL: &'static str = "https://www.71ge.com";
    const SEARCH_URL: &'static str = "https://www.71ge.com/search.php";
}

#[async_trait]
impl Provider for N71geProvider {
    fn name(&self) -> &str {
        "n71ge"
    }

    fn base_url(&self) -> &str {
        Self::BASE_URL
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
        let html = client
            .post_form(Self::SEARCH_URL, &[
                ("s", keyword),
                ("action", "login"),
                ("submit", " 搜 索 "),
            ])
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("tr").unwrap();
        let td_a_sel = Selector::parse("td:first-child a").unwrap();

        for elem in doc.select(&sel) {
            let href = select_attr_in(&elem, "td:first-child a", "href");
            if href.is_empty() {
                continue;
            }

            let book_id = href.trim_matches('/').to_string();
            let title = elem.select(&td_a_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();

            let td_sel2 = Selector::parse("td:nth-child(2) a").unwrap();
            let td_sel3 = Selector::parse("td:nth-child(3)").unwrap();
            let td_sel4 = Selector::parse("td:nth-child(4)").unwrap();
            let td_sel5 = Selector::parse("td:nth-child(5)").unwrap();

            let latest_chapter = elem.select(&td_sel2).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let author = elem.select(&td_sel3).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let word_count = elem.select(&td_sel4).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let update_date = elem.select(&td_sel5).next()
                .map(|e| element_text(&e)).unwrap_or_default();

            results.push(SearchResult {
                site: self.name().to_string(),
                book_id,
                title,
                author,
                latest_chapter,
                update_date,
                word_count,
            });

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", Self::BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();

        info.book_name = select_attr(&doc, "meta[name=\"og:novel:book_name\"]", "content");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "div.introduce h1");
        }

        info.author = select_attr(&doc, "meta[name=\"og:novel:author\"]", "content");
        if info.author.is_empty() {
            info.author = select_attr(&doc, "meta[name=\"author\"]", "content");
        }

        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&doc, "div.pic img", "src");
        }

        info.serial_status = meta_content(&doc, "og:novel:status");
        info.update_time = select_attr(&doc, "meta[name=\"og:novel:update_time\"]", "content");

        info.summary = meta_content(&doc, "og:description");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "div.introduce p.jj");
        }

        // Parse chapters
        let mut chapters = Vec::new();
        let sel = Selector::parse("div.ml_list ul li a").unwrap();
        for elem in doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title = elem.text().next().unwrap_or("").trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let chapter_id = href.rsplit('/').next().unwrap_or("")
                    .split('.').next().unwrap_or("").to_string();
                chapters.push(ChapterInfo {
                    title,
                    chapter_id,
                    url: normalize_url(Self::BASE_URL, href),
                });
            }
        }

        if !chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: "正文".to_string(),
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
        let mut title = String::new();
        let mut all_paragraphs: Vec<String> = Vec::new();
        let mut page = 1;

        loop {
            let url = if page == 1 {
                format!("{}/{}/{}.html", Self::BASE_URL, book_id, chapter_id)
            } else {
                format!("{}/{}/{}_{}.html", Self::BASE_URL, book_id, chapter_id, page)
            };

            let html = client.get(&url).await?;
            let doc = Html::parse_document(&html);

            if title.is_empty() {
                title = select_text(&doc, "div#nr_content div.nr_title h3");
            }

            let sel = Selector::parse("div#nr_content div.novelcontent p").unwrap();
            for elem in doc.select(&sel) {
                let txt = element_text(&elem);
                if !txt.is_empty() {
                    all_paragraphs.push(txt);
                }
            }

            // Remove trailing pagination markers
            if let Some(last) = all_paragraphs.last() {
                if last.starts_with("本章未完") || last.starts_with("本章已完") {
                    all_paragraphs.pop();
                }
            }

            // Check for next page
            let next_page = page + 1;
            let next_suffix = format!("{}_{}.html", chapter_id, next_page);
            if html.contains(&next_suffix) {
                page = next_page;
            } else {
                break;
            }

            if page > 20 {
                break;
            }
        }

        let content = clean_content(&all_paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(N71geProvider)
}
