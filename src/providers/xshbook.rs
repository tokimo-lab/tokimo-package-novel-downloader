use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct XshbookProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(XshbookProvider)
}

const BASE_URL: &str = "https://www.xshbook.com";

fn is_ad_line(text: &str) -> bool {
    let ad_patterns = ["谨记我们的网址", "温馨提示"];
    for pat in &ad_patterns {
        if text.contains(pat) {
            return true;
        }
    }
    // Short lines starting with 提示 or 分享
    if text.starts_with("提示") && text.len() < 100 {
        return true;
    }
    if text.starts_with("分享") && text.len() < 80 {
        return true;
    }
    false
}

#[async_trait]
impl Provider for XshbookProvider {
    fn name(&self) -> &str {
        "xshbook"
    }

    fn display_name(&self) -> &str {
        "小说虎"
    }

    fn base_url(&self) -> &str {
        BASE_URL
    }

    fn supports_search(&self) -> bool {
        false
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // book_id uses "-" as separator (e.g., "95071-95071941"), convert to "/" for URL
        let url_path = book_id.replace('-', "/");
        let url = format!("{}/{}/", BASE_URL, url_path);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "#info h1");
        info.author = select_text(&doc, "#info p:first-of-type")
            .replace('\u{00a0}', "")
            .replace("作者:", "")
            .trim()
            .to_string();
        info.update_time = meta_content(&doc, "og:novel:update_time");
        info.cover_url = select_attr(&doc, "#fmimg img", "src");

        // Summary
        let mut summary = select_text(&doc, "#intro p");
        if let Some(idx) = summary.find("本站提示") {
            summary.truncate(idx);
        }
        info.summary = summary.trim().to_string();

        // Chapters
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list dd a[href]") {
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
                        url: normalize_url(BASE_URL, href),
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
        let url_path = book_id.replace('-', "/");
        let url = format!("{}/{}/{}.html", BASE_URL, url_path, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut title = select_text(&doc, "div.bookname h1");
        if title.is_empty() {
            title = select_text(&doc, "div.con_top");
        }

        let inline_ad_re = Regex::new(r"(本文搜|搜索[:：]?)\s*.*?\s*(免费阅读|本文免费阅读)").ok();

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#content p") {
            for elem in doc.select(&sel) {
                let mut text = element_text(&elem);
                if let Some(ref re) = inline_ad_re {
                    text = re.replace_all(&text, "").to_string();
                }
                let text = text.trim().to_string();
                if !text.is_empty() && !is_ad_line(&text) {
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
