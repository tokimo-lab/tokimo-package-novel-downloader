use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct Jpxs123Provider;

impl Jpxs123Provider {
    const BASE_URL: &'static str = "https://www.jpxs123.com";
    const SEARCH_URL: &'static str = "https://www.jpxs123.com/e/search/indexsearch.php";
}

#[async_trait]
impl Provider for Jpxs123Provider {
    fn name(&self) -> &str {
        "jpxs123"
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
        info.author = select_text(&doc, "div.date span:first-child a");
        info.update_time = select_text(&doc, "div.date span:nth-child(2)")
            .replace("时间：", "")
            .trim()
            .to_string();

        let cover_rel = select_attr(&doc, "div.pic img", "src");
        info.cover_url = if !cover_rel.is_empty() && !cover_rel.starts_with("http") {
            format!("{}{}", Self::BASE_URL, cover_rel)
        } else {
            cover_rel
        };

        info.summary = select_text(&doc, "div.infos p");

        let mut chapters = Vec::new();
        let sel = Selector::parse("div.book_list li a").unwrap();
        for elem in doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
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
    Box::new(Jpxs123Provider)
}
