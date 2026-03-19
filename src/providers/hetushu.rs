use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct HetushuProvider;

#[async_trait]
impl Provider for HetushuProvider {
    fn name(&self) -> &str {
        "hetushu"
    }

    fn display_name(&self) -> &str {
        "和图书"
    }

    fn base_url(&self) -> &str {
        "https://www.hetushu.com"
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
            "https://www.hetushu.com/search/?keyword={}",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("dl.list#body dd") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "h4 a", "href");
                if href.is_empty() {
                    continue;
                }

                // "/book/7631/index.html" -> "7631"
                let book_id = href
                    .trim_end_matches("/index.html")
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .to_string();

                let title = select_text_in(&elem, "h4 a");

                // Author from span, strip "/" delimiters
                let author_raw = select_text_in(&elem, "h4 span");
                let author = author_raw.trim_matches('/').trim().to_string();

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://www.hetushu.com/book/{}/index.html", book_id);
        let catalog_url = format!("https://www.hetushu.com/book/{}/dir.json", book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_json = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&info_doc, "div.book_info h2");
        info.author = select_text(&info_doc, "div.book_info div a");

        let cover_path = select_attr(&info_doc, "div.book_info img", "src");
        info.cover_url = if cover_path.starts_with('/') {
            format!("https://www.hetushu.com{}", cover_path)
        } else {
            cover_path
        };

        // Check serial status from class
        let cls_attr = select_attr(&info_doc, "div.book_info", "class");
        info.serial_status = if cls_attr.contains("finish") {
            "已完结".to_string()
        } else {
            "连载中".to_string()
        };

        info.summary = select_text(&info_doc, "div.intro p");

        // Parse catalog JSON: array of [tag, text, chapter_id?]
        if let Ok(catalog_data) = serde_json::from_str::<Vec<Vec<String>>>(&catalog_json) {
            let mut current_vol_name = "未命名卷".to_string();
            let mut current_chapters: Vec<ChapterInfo> = Vec::new();

            for elem in &catalog_data {
                if elem.is_empty() {
                    continue;
                }
                match elem[0].as_str() {
                    "dt" => {
                        if !current_chapters.is_empty() {
                            info.volumes.push(Volume {
                                volume_name: current_vol_name.clone(),
                                chapters: std::mem::take(&mut current_chapters),
                            });
                        }
                        if elem.len() > 1 {
                            current_vol_name = elem[1].trim().to_string();
                        }
                    }
                    "dd" => {
                        if elem.len() > 2 {
                            let title = elem[1].trim().to_string();
                            let chapter_id = elem[2].clone();
                            current_chapters.push(ChapterInfo {
                                title,
                                chapter_id: chapter_id.clone(),
                                url: format!(
                                    "https://www.hetushu.com/book/{}/{}.html",
                                    book_id, chapter_id
                                ),
                            });
                        }
                    }
                    _ => {}
                }
            }

            if !current_chapters.is_empty() {
                info.volumes.push(Volume {
                    volume_name: current_vol_name,
                    chapters: current_chapters,
                });
            }
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
            "https://www.hetushu.com/book/{}/{}.html",
            book_id, chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "#content h2.h2");

        // Extract paragraphs from #content divs
        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content div") {
            let paragraphs: Vec<String> = doc
                .select(&sel)
                .map(|elem| element_text(&elem))
                .filter(|t| !t.is_empty())
                .collect();
            content = paragraphs.join("\n");
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(HetushuProvider)
}
