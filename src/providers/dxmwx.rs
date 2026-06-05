use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct DxmwxProvider;

#[async_trait]
impl Provider for DxmwxProvider {
    fn name(&self) -> &str {
        "dxmwx"
    }

    fn display_name(&self) -> &str {
        "大熊猫文学网"
    }

    fn base_url(&self) -> &str {
        "https://www.dxmwx.org"
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
            "https://www.dxmwx.org/list/{}.html",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("#ListContents div[style*='position']") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "div.margin0h5 a:first-child", "href");
                if href.is_empty() {
                    continue;
                }

                let book_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let title = select_text_in(&elem, "div.margin0h5 a:first-child");
                let author = select_text_in(&elem, "div.margin0h5 a:nth-child(2)");

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
        let info_url = format!("https://www.dxmwx.org/book/{}.html", book_id);
        let catalog_url = format!("https://www.dxmwx.org/chapter/{}.html", book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "span[style*='font-size: 24px']");

        // Author from link in the header area
        info.author = select_text(&info_doc, "div[style*='height: 28px'] a");

        let cover_path = select_attr(&info_doc, "img.imgwidth", "src");
        info.cover_url = if cover_path.starts_with('/') {
            format!("https://www.dxmwx.org{}", cover_path)
        } else {
            cover_path
        };

        info.summary = select_text(&info_doc, "div[style*='border-bottom'] div");

        // Parse chapters from catalog page
        let mut chapters = Vec::new();
        if let Ok(sel) =
            Selector::parse("div[style*='height:40px'][style*='border-bottom'] a[href]")
        {
            for elem in catalog_doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() || href.is_empty() {
                        continue;
                    }
                    // "/read/57215_50197663.html" -> "50197663"
                    let chapter_id = href
                        .split("read/")
                        .last()
                        .unwrap_or("")
                        .split(".html")
                        .next()
                        .unwrap_or("")
                        .split('_')
                        .next_back()
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
        let url = format!("https://www.dxmwx.org/read/{}_{}.html", book_id, chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "#ChapterTitle");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#Lab_Contents p") {
            let paragraphs: Vec<String> = doc
                .select(&sel)
                .map(|p| element_text(&p))
                .filter(|t| !t.is_empty())
                .collect();
            content = paragraphs.join("\n");
        }

        if content.is_empty() {
            if let Ok(sel) = Selector::parse("#Lab_Contents") {
                if let Some(elem) = doc.select(&sel).next() {
                    content = html_to_text(&elem.inner_html());
                }
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
    Box::new(DxmwxProvider)
}
