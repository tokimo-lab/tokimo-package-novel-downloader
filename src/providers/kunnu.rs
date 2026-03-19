use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct KunnuProvider;

#[async_trait]
impl Provider for KunnuProvider {
    fn name(&self) -> &str {
        "kunnu"
    }

    fn display_name(&self) -> &str {
        "鲲弩小说"
    }

    fn base_url(&self) -> &str {
        "https://www.kunnu.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", self.base_url(), book_id);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Cover
        info.cover_url = select_attr(&doc, "div.book-img img", "src");
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = normalize_url(self.base_url(), &info.cover_url);
        }

        // Book name
        if let Ok(sel) = Selector::parse("div.book-describe h1, div[class*='book-describe'] h1") {
            if let Some(elem) = doc.select(&sel).next() {
                info.book_name = element_text(&elem);
            }
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Parse metadata from p tags in book-describe
        if let Ok(sel) = Selector::parse("div.book-describe p, div[class*='book-describe'] p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("作者") {
                    info.author = text.replace("作者：", "").replace("作者:", "").trim().to_string();
                } else if text.contains("状态") {
                    info.serial_status = text.replace("状态：", "").replace("状态:", "").trim().to_string();
                } else if text.contains("最近更新") {
                    info.update_time = text.replace("最近更新：", "").replace("最近更新:", "").trim().to_string();
                }
            }
        }

        // Summary from describe-html
        if let Ok(sel) = Selector::parse("div.describe-html p") {
            let mut summary_parts = Vec::new();
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if !text.is_empty() {
                    summary_parts.push(text);
                }
            }
            info.summary = summary_parts.join("\n");
        }

        // Parse volumes and chapters from #content-list
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        if let Ok(sel) = Selector::parse("#content-list > div") {
            for elem in doc.select(&sel) {
                let class = elem.value().attr("class").unwrap_or("");

                if class.contains("title") {
                    // Volume header
                    if !current_chapters.is_empty() {
                        volumes.push(Volume {
                            volume_name: current_volume_name.clone(),
                            chapters: std::mem::take(&mut current_chapters),
                        });
                    }
                    current_volume_name = element_text(&elem);
                } else if class.contains("book-list") {
                    // Chapter list
                    if let Ok(a_sel) = Selector::parse("a[href]") {
                        for a_elem in elem.select(&a_sel) {
                            if let Some(href) = a_elem.value().attr("href") {
                                let title = element_text(&a_elem);
                                if title.is_empty() {
                                    continue;
                                }
                                let chapter_id = href
                                    .trim_end_matches(".htm")
                                    .trim_end_matches(".html")
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or("")
                                    .to_string();
                                current_chapters.push(ChapterInfo {
                                    title,
                                    chapter_id,
                                    url: normalize_url(self.base_url(), href),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Flush remaining chapters
        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_volume_name,
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
        let url = format!(
            "{}/{}/{}.htm",
            self.base_url(),
            book_id,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "#nr_title, h1.post-title, h1");

        // Content from #nr1 p, filtering ads
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#nr1 p") {
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
        "鲲弩小说", "kunnu.com", "最新章节", "请记住",
        "本站域名", "请收藏", "加入书架",
    ];
    let lower = text.to_lowercase();
    ad_keywords.iter().any(|kw| lower.contains(kw))
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(KunnuProvider)
}
