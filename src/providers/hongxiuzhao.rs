use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct HongxiuzhaoProvider;

#[async_trait]
impl Provider for HongxiuzhaoProvider {
    fn name(&self) -> &str {
        "hongxiuzhao"
    }

    fn display_name(&self) -> &str {
        "红袖招"
    }

    fn base_url(&self) -> &str {
        "https://hongxiuzhao.net"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}.html", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Title
        info.book_name = select_text(&doc, "div.m-bookdetail h1, div[class*='m-bookdetail'] h1");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author
        info.author = select_text(&doc, "p.author a, p[class*='author'] a");

        // Cover
        let mut cover = select_attr(&doc, "a.cover img, a[class*='cover'] img", "src");
        if !cover.is_empty() && !cover.starts_with("http") {
            if cover.starts_with("//") {
                cover = format!("https:{}", cover);
            } else {
                cover = format!("{}{}", self.base_url(), cover);
            }
        }
        info.cover_url = cover;

        // Summary
        info.summary = select_text(&doc, "p.summery, p[class*='summery']");

        // Chapters from section.yd-chapter ul a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("section.yd-chapter ul a[href], section[class*='yd-chapter'] ul a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
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
                volume_name: String::new(),
                chapters,
            });
        }

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/{}.html", self.base_url(), chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "div.article-content h1");
        if title.is_empty() {
            let _ = select_text(&doc, "h1");
        }

        // Content from div.article-content p, filtering ads
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.article-content p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.is_empty() {
                    continue;
                }
                if is_ad_line(&text) {
                    continue;
                }
                paragraphs.push(text);
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

fn is_ad_line(text: &str) -> bool {
    let ad_keywords = [
        "红袖招", "hongxiuzhao", "本站域名", "请记住",
        "最新章节", "请收藏", "加入书架", "www.",
    ];
    let lower = text.to_lowercase();
    ad_keywords.iter().any(|kw| lower.contains(kw))
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(HongxiuzhaoProvider)
}
