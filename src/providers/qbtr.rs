use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct QbtrProvider;

impl QbtrProvider {
    const BASE_URL: &'static str = "https://www.qbtr.cc";
    const SEARCH_URL: &'static str = "https://www.qbtr.cc/e/search/index.php";
}

#[async_trait]
impl Provider for QbtrProvider {
    fn name(&self) -> &str {
        "qbtr"
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
            .post_form(
                Self::SEARCH_URL,
                &[("keyboard", keyword), ("show", "title"), ("classid", "0")],
            )
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("div.books.m-cols div.bk").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let href = select_attr_in(&elem, "h3 a", "href");
            if href.is_empty() {
                continue;
            }

            // '/tongren/8850.html' -> "tongren-8850"
            let book_id = href
                .trim_matches('/')
                .split('.')
                .next()
                .unwrap_or("")
                .replace('/', "-");

            let title = select_text_in(&elem, "h3 a");
            let author = select_text_in(&elem, "div.booknews")
                .replace("作者：", "")
                .trim()
                .to_string();
            let update_date = select_text_in(&elem, "div.booknews label.date");

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

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let real_id = book_id.replace('-', "/");
        let url = format!("{}/{}.html", Self::BASE_URL, real_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.infos h1");

        // Author & update_time from regex on date div
        let date_text = select_text(&doc, "div.date");
        if let Ok(re) = Regex::new(r"作者[：:]\s*([^日]+)") {
            if let Some(cap) = re.captures(&date_text) {
                info.author = cap[1].trim().to_string();
            }
        }
        if let Ok(re) = Regex::new(r"日期[：:]\s*([\d-]+)") {
            if let Some(cap) = re.captures(&date_text) {
                info.update_time = cap[1].to_string();
            }
        }

        // Summary
        let mut paras = Vec::new();
        let p_sel = Selector::parse("div.infos p").unwrap();
        for elem in doc.select(&p_sel) {
            let txt = element_text(&elem);
            if !txt.is_empty() {
                paras.push(txt);
            }
        }
        info.summary = paras.join("\n");

        // Chapters
        let chapter_re = Regex::new(r"^/[^/]+/\d+/(\d+)\.html$").ok();
        let mut chapters = Vec::new();
        let sel = Selector::parse("div.book_list li a").unwrap();
        for elem in doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title = element_text(&elem);
                if title.is_empty() {
                    continue;
                }
                let chapter_id = chapter_re
                    .as_ref()
                    .and_then(|re| re.captures(href))
                    .map(|cap| cap[1].to_string())
                    .unwrap_or_default();
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
        let real_id = book_id.replace('-', "/");
        let url = format!("{}/{}/{}.html", Self::BASE_URL, real_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let raw_title = select_text(&doc, "div.read_chapterName h1");
        let book_name = {
            let sel = Selector::parse("div.readTop a").unwrap();
            doc.select(&sel)
                .next_back()
                .map(|e| element_text(&e))
                .unwrap_or_default()
        };
        let title = raw_title.replace(&book_name, "").trim().to_string();

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("div.read_chapterDetail p").unwrap();
        for elem in doc.select(&sel) {
            let txt = element_text(&elem);
            if !txt.is_empty() {
                paragraphs.push(txt);
            }
        }

        let content = clean_content(&paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(QbtrProvider)
}
