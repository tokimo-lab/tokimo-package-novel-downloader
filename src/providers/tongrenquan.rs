use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct TongrenquanProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(TongrenquanProvider)
}

#[async_trait]
impl Provider for TongrenquanProvider {
    fn name(&self) -> &str {
        "tongrenquan"
    }

    fn display_name(&self) -> &str {
        "同人圈"
    }

    fn base_url(&self) -> &str {
        "https://www.tongrenquan.org"
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
                "https://www.tongrenquan.org/e/search/indexstart.php",
                &[("keyboard", keyword), ("show", "title"), ("classid", "0")],
            )
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.books.m-cols div.bk") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "h3 a[href]", "href");
                if href.is_empty() {
                    continue;
                }
                let book_id = href
                    .split('/')
                    .last()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let title = select_text_in(&elem, "div.bk_right h3 a");
                let author =
                    select_text_in(&elem, "div.bk_right div.booknews").replace("作者：", "");
                let update_date = select_text_in(&elem, "div.bk_right div.booknews label.date");

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
        let url = format!("https://www.tongrenquan.org/tongren/{}.html", book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.infos h1");
        info.author = select_text(&doc, "div.date span").replace("作者：", "");
        info.cover_url = format!(
            "{}{}",
            self.base_url(),
            select_attr(&doc, "div.pic img", "src")
        );

        let paras: Vec<String> = if let Ok(sel) = Selector::parse("div.infos p") {
            doc.select(&sel)
                .map(|e| element_text(&e))
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        };
        info.summary = paras.join("\n");

        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("div.book_list ul li a[href]") {
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
            "https://www.tongrenquan.org/tongren/{}/{}.html",
            book_id, chapter_id
        );
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let raw_title = select_text(&doc, "div.read_chapterName h1");
        let book_name = {
            if let Ok(sel) = Selector::parse("div.readTop a") {
                doc.select(&sel)
                    .last()
                    .map(|e| element_text(&e))
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };
        let title = raw_title.replace(&book_name, "").trim().to_string();

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.read_chapterDetail p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
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
