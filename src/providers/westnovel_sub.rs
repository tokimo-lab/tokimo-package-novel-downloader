use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 西方奇幻小说网 (sub-site) provider
pub struct WestnovelSubProvider;

impl WestnovelSubProvider {
    fn transform_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for WestnovelSubProvider {
    fn name(&self) -> &str {
        "westnovel_sub"
    }

    fn display_name(&self) -> &str {
        "西方奇幻小说网(子站)"
    }

    fn base_url(&self) -> &str {
        "https://www.westnovel.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let real_id = Self::transform_book_id(book_id);
        let url = format!("https://www.westnovel.com/{}.html", real_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "#bookinfo h1");
        info.author = select_text(&doc, "#count li a");
        // Filter to the li containing "作者"
        if let Ok(li_sel) = Selector::parse("#count li") {
            for li in doc.select(&li_sel) {
                let text = element_text(&li);
                if text.contains("作者") {
                    if let Ok(a_sel) = Selector::parse("a") {
                        if let Some(a) = li.select(&a_sel).next() {
                            info.author = element_text(&a);
                        }
                    }
                    break;
                }
            }
        }

        let cover_path = select_attr(&doc, "#bookimg img", "src");
        if !cover_path.is_empty() {
            info.cover_url = format!("{}{}", self.base_url(), cover_path);
        }

        info.summary = select_text(&doc, "#bookintro");

        // Chapter list
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#chapterList a[href]") {
            for a in doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("").trim();
                let title = element_text(&a);
                if href.is_empty() || title.is_empty() {
                    continue;
                }
                // href like: /q/showinfo-2-22999-0.html
                let chapter_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .replace("showinfo-", "");
                chapters.push(ChapterInfo {
                    title,
                    chapter_id,
                    url: normalize_url(self.base_url(), href),
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
        let real_id = Self::transform_book_id(book_id);
        // Extract prefix from book_id (first segment before /)
        let prefix = real_id.split('/').next().unwrap_or(&real_id);
        let url = format!(
            "https://www.westnovel.com/{}/showinfo-{}.html",
            prefix, chapter_id
        );
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "#mlfy_main_text h1");

        // Try paragraph extraction first, fallback to raw text
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#TextContent p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                if !text.is_empty() {
                    paragraphs.push(text);
                }
            }
        }

        if paragraphs.is_empty() {
            if let Ok(sel) = Selector::parse("#TextContent") {
                if let Some(elem) = doc.select(&sel).next() {
                    let text = html_to_text(&elem.inner_html());
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                }
            }
        }

        let content = paragraphs.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(WestnovelSubProvider)
}
