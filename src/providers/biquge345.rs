use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct Biquge345Provider;

#[async_trait]
impl Provider for Biquge345Provider {
    fn name(&self) -> &str {
        "biquge345"
    }

    fn display_name(&self) -> &str {
        "笔趣阁(345)"
    }

    fn base_url(&self) -> &str {
        "https://www.biquge345.com"
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
                "https://www.biquge345.com/s.php",
                &[("type", "articlename"), ("s", keyword), ("submit", "")],
            )
            .await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("ul.search li:not([class])") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "span.name a", "href");
                if href.is_empty() {
                    continue;
                }

                let book_id = href
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .to_string();

                let title = select_text_in(&elem, "span.name a");
                let latest_chapter = select_text_in(&elem, "span.jie a");
                let author = select_text_in(&elem, "span.zuo a");
                let update_date = select_text_in(&elem, "span.time");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date,
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.biquge345.com/book/{}/", book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.right_border h1");
        info.author = select_text(&doc, "div.xinxi span.x1:first-of-type a");

        let mut cover = select_attr(&doc, "div.zhutu img", "src");
        if cover.starts_with("//") {
            cover = format!("https:{}", cover);
        } else if cover.starts_with('/') {
            cover = format!("{}{}", self.base_url(), cover);
        }
        info.cover_url = cover;

        let update_raw = select_text(&doc, "div.xinxi span.x2:first-of-type");
        info.update_time = update_raw.replace("更新时间：", "").trim().to_string();

        info.summary = select_text(&doc, "div.xinxi div.x3");

        // Parse chapters
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.border ul.info a[href]") {
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
        let url = format!(
            "https://www.biquge345.com/chapter/{}/{}.html",
            book_id, chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "#neirong h1");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#txt") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: clean_content(&content),
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge345Provider)
}
