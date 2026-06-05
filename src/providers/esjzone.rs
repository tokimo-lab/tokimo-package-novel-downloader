use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct EsjzoneProvider;

#[async_trait]
impl Provider for EsjzoneProvider {
    fn name(&self) -> &str {
        "esjzone"
    }

    fn display_name(&self) -> &str {
        "ESJ Zone"
    }

    fn base_url(&self) -> &str {
        "https://www.esjzone.cc"
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
            "https://www.esjzone.cc/tags/{}/",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.card-body") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "h5.card-title a:first-child", "href");
                if href.is_empty() {
                    continue;
                }

                // href format: /detail/<book_id>.html
                let book_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let title = select_text_in(&elem, "h5.card-title a:first-child");
                let latest_chapter = select_text_in(&elem, "div.card-ep a:first-child");
                let author = select_text_in(&elem, "div.card-author a:first-child");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.esjzone.cc/detail/{}.html", book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "h2.text-normal");
        info.author = select_text(&doc, "li:has(strong) a");
        info.cover_url = select_attr(&doc, "div.product-gallery img", "src");
        info.summary = select_text(&doc, "div.description");

        // Parse chapters from #chapterList
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#chapterList a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    if !href.contains("www.esjzone.cc") {
                        continue;
                    }
                    let title_attr = elem.value().attr("data-title").unwrap_or("");
                    let title = if !title_attr.is_empty() {
                        title_attr.trim().to_string()
                    } else {
                        element_text(&elem)
                    };
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
                        url: href.to_string(),
                    });
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
        let url = format!(
            "https://www.esjzone.cc/forum/{}/{}.html",
            book_id, chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "h2");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("div.forum-content") {
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
    Box::new(EsjzoneProvider)
}
