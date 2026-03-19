use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct I25zwProvider;

#[async_trait]
impl Provider for I25zwProvider {
    fn name(&self) -> &str {
        "i25zw"
    }

    fn display_name(&self) -> &str {
        "25中文网"
    }

    fn base_url(&self) -> &str {
        "https://www.i25zw.com"
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
                "https://www.i25zw.com/search.html",
                &[
                    ("searchkey", keyword),
                    ("searchtype", "all"),
                    ("Submit", ""),
                ],
            )
            .await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("#alistbox") {
            for elem in doc.select(&sel).take(limit) {
                let book_url = select_attr_in(&elem, "div.pic a", "href");
                if book_url.is_empty() {
                    continue;
                }

                // "https://www.i25zw.com/book/309209.html" -> "309209"
                let book_id = book_url
                    .rsplit('/')
                    .next()
                    .unwrap_or(&book_url)
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let title = select_text_in(&elem, "div.title h2 a");
                let author_raw = select_text_in(&elem, "div.title span");
                let author = author_raw.replace("作者：", "").trim().to_string();
                let latest_chapter = select_text_in(&elem, "div.sys li:first-child a");

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
        let info_url = format!("https://www.i25zw.com/book/{}.html", book_id);
        let catalog_url = format!("https://www.i25zw.com/{}/", book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&info_doc, "h1.f21h");
        info.author = select_text(&info_doc, "h1.f21h em a");
        info.cover_url = select_attr(&info_doc, "div.pic img", "src");
        info.summary = select_text(&info_doc, "div.intro[style]");

        // Parse chapters from catalog page - #list dl dd a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list dl dd a[href]") {
            for elem in catalog_doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    // '/311006/252845677.html' -> '252845677'
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
        let url = format!(
            "https://www.i25zw.com/{}/{}.html",
            book_id, chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "div.zhangjieming h1");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content p") {
            let paragraphs: Vec<String> = doc
                .select(&sel)
                .map(|p| element_text(&p))
                .filter(|t| !t.is_empty())
                .collect();
            content = paragraphs.join("\n");
        }

        if content.is_empty() {
            if let Ok(sel) = Selector::parse("#content") {
                if let Some(elem) = doc.select(&sel).next() {
                    content = html_to_text(&elem.inner_html());
                }
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
    Box::new(I25zwProvider)
}
