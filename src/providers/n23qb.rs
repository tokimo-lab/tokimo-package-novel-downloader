use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct N23qbProvider;

impl N23qbProvider {
    const BASE_URL: &'static str = "https://www.23qb.com";
    const SEARCH_URL: &'static str = "https://www.23qb.com/search.html";
}

#[async_trait]
impl Provider for N23qbProvider {
    fn name(&self) -> &str {
        "n23qb"
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
        let url = format!(
            "{}?searchkey={}",
            Self::SEARCH_URL,
            urlencoding::encode(keyword)
        );
        let html = client.get(&url).await?;

        // Check if redirected to a single book detail page
        if html.contains("<meta property=\"og:url\"") {
            return self.parse_detail_search(&html);
        }

        self.parse_list_search(&html, limit)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/book/{}/", Self::BASE_URL, book_id);
        let catalog_url = format!("{}/book/{}/catalog", Self::BASE_URL, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "h1.page-title");
        info.author = select_attr(&info_doc, "a[href*=\"/author/\"]", "title");
        info.cover_url = select_attr(&info_doc, "div.novel-cover img", "data-src");
        info.serial_status = select_text(&info_doc, "a.tag-link");
        info.summary = select_text(&info_doc, "div.novel-info-item.novel-info-content span");

        // Parse volumes from catalog
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        let sel = Selector::parse("div.box > h2, div.box > div.module-row-info").unwrap();
        let a_sel = Selector::parse("a.module-row-text").unwrap();
        let span_sel = Selector::parse("span").unwrap();

        for elem in catalog_doc.select(&sel) {
            let tag = elem.value().name();
            let class_attr = elem.value().attr("class").unwrap_or("");

            if tag == "h2" && class_attr.contains("module-title") {
                if !current_chapters.is_empty() || !current_volume_name.is_empty() {
                    volumes.push(Volume {
                        volume_name: current_volume_name.clone(),
                        chapters: std::mem::take(&mut current_chapters),
                    });
                }
                current_volume_name = element_text(&elem);
            } else if tag == "div" && class_attr.contains("module-row-info") {
                if let Some(a) = elem.select(&a_sel).next() {
                    let href = a.value().attr("href").unwrap_or("");
                    if href == "javascript:cid(0)" || href.is_empty() {
                        continue;
                    }
                    let title = a
                        .select(&span_sel)
                        .next()
                        .map(|s| element_text(&s))
                        .unwrap_or_default();
                    let chapter_id = href.rsplit('/').next().unwrap_or("").replace(".html", "");
                    current_chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(Self::BASE_URL, href),
                    });
                }
            }
        }

        if !current_chapters.is_empty() || !current_volume_name.is_empty() {
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
        let url = format!("{}/book/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1.article-title");

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("div.article-content p").unwrap();
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

impl N23qbProvider {
    fn parse_detail_search(&self, html: &str) -> Result<Vec<SearchResult>> {
        let doc = Html::parse_document(html);

        let book_url = meta_content(&doc, "og:url");
        if book_url.is_empty() {
            return Ok(vec![]);
        }

        let book_id = book_url
            .split("book/")
            .last()
            .unwrap_or("")
            .trim_matches('/')
            .to_string();
        let title = select_text(&doc, "h1.page-title");
        let author = select_attr(&doc, "a[href*=\"/author/\"]", "title");

        Ok(vec![SearchResult {
            site: self.name().to_string(),
            book_id,
            title,
            author,
            latest_chapter: String::new(),
            update_date: String::new(),
            word_count: String::new(),
        }])
    }

    fn parse_list_search(&self, html: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let doc = Html::parse_document(html);
        let mut results = Vec::new();

        let sel = Selector::parse("div.module-search-item").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let href = select_attr_in(&elem, "div.novel-info-header h3 a", "href");
            if href.is_empty() {
                continue;
            }

            let book_id = href
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();
            let title = select_text_in(&elem, "div.novel-info-header h3 a");

            results.push(SearchResult {
                site: self.name().to_string(),
                book_id,
                title,
                author: String::new(),
                latest_chapter: String::new(),
                update_date: String::new(),
                word_count: String::new(),
            });
        }

        Ok(results)
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(N23qbProvider)
}
