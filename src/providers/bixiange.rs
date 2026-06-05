use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct BixiangeProvider;

const SPECIAL_PREFIXES: &[&str] = &["cyjk", "khjj", "guanchang"];

impl BixiangeProvider {
    fn is_special_prefix(prefix: &str) -> bool {
        SPECIAL_PREFIXES.contains(&prefix)
    }
}

#[async_trait]
impl Provider for BixiangeProvider {
    fn name(&self) -> &str {
        "bixiange"
    }

    fn display_name(&self) -> &str {
        "笔仙阁"
    }

    fn base_url(&self) -> &str {
        "https://m.bixiange.me"
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
                "https://m.bixiange.me/e/search/indexpage.php",
                &[("keyboard", keyword), ("show", "title"), ("classid", "0")],
            )
            .await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.list li") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "div.cover a", "href");
                if href.is_empty() {
                    continue;
                }

                // "/khjj/11945.html" -> "khjj-11945"
                let href_path = href.trim_matches('/').split('.').next().unwrap_or("");
                let book_id = href_path.replace('/', "-");

                let title = select_text_in(&elem, "div.title a");

                let update_date_raw = select_text_in(&elem, "div.tips span");
                let update_date = update_date_raw
                    .split("时间：")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author: String::new(),
                    latest_chapter: String::new(),
                    update_date,
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let book_id_path = book_id.replace('-', "/");
        let prefix = book_id_path.split('/').next().unwrap_or("");

        let url = if Self::is_special_prefix(prefix) {
            format!("https://m.bixiange.me/{}.html", book_id_path)
        } else {
            format!("https://m.bixiange.me/{}/", book_id_path)
        };

        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.desc h1");

        let author_raw = select_text(&doc, "div.descTip span");
        if author_raw.contains("作者") {
            info.author = author_raw.replace("作者：", "").trim().to_string();
        }

        let mut cover = select_attr(&doc, "div.cover img", "src");
        if cover.starts_with("//") {
            cover = format!("https:{}", cover);
        } else if cover.starts_with('/') {
            cover = format!("{}{}", self.base_url(), cover);
        }
        info.cover_url = cover;

        info.summary = select_text(&doc, "div.descInfo");

        // Parse chapters
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.catalog a[href]") {
            for elem in doc.select(&sel) {
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
        let book_id_path = book_id.replace('-', "/");
        let prefix = book_id_path.split('/').next().unwrap_or("");

        let url = if Self::is_special_prefix(prefix) {
            format!("https://m.bixiange.me/{}/{}.html", book_id_path, chapter_id)
        } else {
            format!(
                "https://m.bixiange.me/{}/index/{}.html",
                book_id_path, chapter_id
            )
        };

        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "div.article h1");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#mycontent") {
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
    Box::new(BixiangeProvider)
}
