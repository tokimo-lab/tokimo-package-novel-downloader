use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct N37yqProvider;

impl N37yqProvider {
    const BASE_URL: &'static str = "https://www.37yq.com";
    const SEARCH_URL: &'static str = "https://www.37yq.com/so.html";
}

#[async_trait]
impl Provider for N37yqProvider {
    fn name(&self) -> &str {
        "n37yq"
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
                ("searchkey", keyword),
                ("searchtype", "all"),
            ])
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("div.search-tab div.search-result-list").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let book_url = {
                let h = select_attr_in(&elem, "h2.tit a", "href");
                if h.is_empty() {
                    select_attr_in(&elem, "div.imgbox a", "href")
                } else {
                    h
                }
            };
            if book_url.is_empty() {
                continue;
            }

            let book_id = book_url.rsplit('/').next().unwrap_or("")
                .split('.').next().unwrap_or("").to_string();

            let cover_url = select_attr_in(&elem, "div.imgbox img", "src");
            let title = select_text_in(&elem, "h2.tit a");
            let author = select_text_in(&elem, "div.bookinfo a:first-child");

            let word_count = select_text_in(&elem, "div.bookinfo span script")
                .replace("towan('", "").replace("')", "");

            results.push(SearchResult {
                site: self.name().to_string(),
                book_id,
                title,
                author,
                latest_chapter: String::new(),
                update_date: String::new(),
                word_count,
            });
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/lightnovel/{}.html", Self::BASE_URL, book_id);
        let catalog_url = format!("{}/lightnovel/{}/catalog", Self::BASE_URL, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = meta_content(&info_doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "h1.book-name");
        }
        info.author = meta_content(&info_doc, "og:novel:author");
        info.cover_url = meta_content(&info_doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&info_doc, "div.book-cover img", "src");
        }
        info.serial_status = meta_content(&info_doc, "og:novel:status");
        info.update_time = meta_content(&info_doc, "og:novel:update_time");

        let summary = meta_content(&info_doc, "og:description")
            .replace('\u{3000}', " ").replace('\u{00a0}', " ");
        info.summary = if summary.is_empty() {
            select_text(&info_doc, "div.book-dec p")
        } else {
            summary
        };

        // Parse volumes from catalog
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name: Option<String> = None;
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        let sel = Selector::parse("ul.chapter-list > div.volume, ul.chapter-list > li").unwrap();
        let a_sel = Selector::parse("a").unwrap();

        for elem in catalog_doc.select(&sel) {
            let tag = elem.value().name();
            let class_attr = elem.value().attr("class").unwrap_or("");

            if tag == "div" && class_attr.contains("volume") {
                // Flush previous volume
                if current_volume_name.is_none() && !current_chapters.is_empty() {
                    volumes.push(Volume {
                        volume_name: "正文".to_string(),
                        chapters: std::mem::take(&mut current_chapters),
                    });
                } else if current_volume_name.is_some() {
                    volumes.push(Volume {
                        volume_name: current_volume_name.take().unwrap_or_default(),
                        chapters: std::mem::take(&mut current_chapters),
                    });
                }
                current_volume_name = Some(element_text(&elem));
            } else if tag == "li" {
                if let Some(a) = elem.select(&a_sel).next() {
                    let href = a.value().attr("href").unwrap_or("");
                    if href.is_empty() {
                        continue;
                    }
                    let title = a.text().next().unwrap_or("").trim().to_string();
                    let chapter_id = href.rsplit('/').next().unwrap_or("")
                        .split('.').next().unwrap_or("").to_string();
                    current_chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(Self::BASE_URL, href),
                    });
                }
            }
        }

        // Flush last volume
        if current_volume_name.is_none() && !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: "正文".to_string(),
                chapters: current_chapters,
            });
        } else if current_volume_name.is_some() {
            volumes.push(Volume {
                volume_name: current_volume_name.unwrap_or_default(),
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
        let url = format!("{}/lightnovel/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut title = select_text(&doc, "div#mlfy_main_text h1");
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("div#TextContent > p").unwrap();
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
    Box::new(N37yqProvider)
}
