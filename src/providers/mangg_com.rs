use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct ManggComProvider;

#[async_trait]
impl Provider for ManggComProvider {
    fn name(&self) -> &str {
        "mangg_com"
    }

    fn display_name(&self) -> &str {
        "追书网"
    }

    fn base_url(&self) -> &str {
        "https://www.mangg.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name
        info.book_name = select_text(&doc, "#info h1");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author from first p in #info
        if let Ok(sel) = Selector::parse("#info p") {
            let ps: Vec<_> = doc.select(&sel).collect();
            if !ps.is_empty() {
                let author_text = element_text(&ps[0]);
                info.author = author_text
                    .replace('\u{00a0}', "")
                    .replace("作者：", "")
                    .replace("作者:", "")
                    .trim()
                    .to_string();
            }
            if ps.len() > 1 {
                let status_text = element_text(&ps[1]);
                info.serial_status = status_text
                    .replace('\u{00a0}', "")
                    .replace("状态：", "")
                    .replace("状态:", "")
                    .replace(',', "")
                    .trim()
                    .to_string();
            }
            if ps.len() > 2 {
                let update_text = element_text(&ps[2]);
                info.update_time = update_text
                    .replace("最后更新：", "")
                    .replace("最后更新:", "")
                    .trim()
                    .to_string();
            }
        }

        // Cover
        let mut cover = select_attr(&doc, "#sidebar img", "src");
        if !cover.is_empty() && !cover.starts_with("http") {
            cover = format!("{}{}", self.base_url(), cover);
        }
        info.cover_url = cover;

        // Summary
        info.summary = select_text(&doc, "#intro");

        // Chapters from #list dd a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list dd a[href]") {
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
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/{}/{}.html", self.base_url(), book_id, chapter_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut title = select_text(&doc, "div.bookname h1");
        if title.is_empty() {
            title = select_text(&doc, "h1");
        }

        // Content from #content, filtering scripts/styles
        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
            }
        }

        // Filter ad lines
        let filtered: Vec<&str> = content.lines().filter(|line| !is_ad_line(line)).collect();
        let content = filtered.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

fn is_ad_line(text: &str) -> bool {
    let ad_keywords = [
        "mangg.com",
        "追书网",
        "本站域名",
        "请记住",
        "最新章节",
        "请收藏",
        "加入书架",
    ];
    let lower = text.to_lowercase();
    ad_keywords.iter().any(|kw| lower.contains(kw))
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(ManggComProvider)
}
