use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct ShencouProvider;

impl ShencouProvider {
    fn chapter_prefix(bid: &str) -> String {
        if bid.len() <= 3 {
            "0".to_string()
        } else {
            bid[..bid.len() - 3].to_string()
        }
    }
}

#[async_trait]
impl Provider for ShencouProvider {
    fn name(&self) -> &str {
        "shencou"
    }

    fn display_name(&self) -> &str {
        "神凑轻小说"
    }

    fn base_url(&self) -> &str {
        "https://www.shencou.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch book info page
        let info_url = format!("{}/books/read_{}.html", self.base_url(), book_id);
        let html_str = client.get(&info_url).await?;
        let doc = Html::parse_document(&html_str);

        let mut info = BookInfo::default();

        // Book name from span a
        if let Ok(sel) = Selector::parse("span a") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if !text.is_empty() {
                    info.book_name = text.trim_end_matches("小说").to_string();
                    break;
                }
            }
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        // Author, word count, status, update from td elements
        if let Ok(sel) = Selector::parse("td") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if text.contains("小说作者") {
                    info.author = text
                        .replace("小说作者：", "")
                        .replace("小说作者:", "")
                        .trim()
                        .to_string();
                } else if text.contains("全文长度") {
                    info.word_count = text
                        .replace("全文长度：", "")
                        .replace("全文长度:", "")
                        .trim()
                        .to_string();
                } else if text.contains("写作进度") {
                    info.serial_status = text
                        .replace("写作进度：", "")
                        .replace("写作进度:", "")
                        .trim()
                        .to_string();
                } else if text.contains("最后更新") {
                    info.update_time = text
                        .replace("最后更新：", "")
                        .replace("最后更新:", "")
                        .trim()
                        .to_string();
                }
            }
        }

        // Cover from image link
        info.cover_url = select_attr(&doc, "a[href*='/files/article/image'] img", "src");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&doc, "img[src*='/files/article/image']", "src");
        }
        if !info.cover_url.is_empty() && !info.cover_url.starts_with("http") {
            info.cover_url = normalize_url(self.base_url(), &info.cover_url);
        }

        // Summary - extract from table cell containing "内容简介"
        if let Ok(sel) = Selector::parse("td[width='80%'][valign='top']") {
            if let Some(elem) = doc.select(&sel).next() {
                let text = element_text(&elem);
                let summary = if let Some(start) = text.find("内容简介：") {
                    let after = &text[start + "内容简介：".len()..];
                    if let Some(end) = after.find("本书公告：") {
                        after[..end].trim().to_string()
                    } else {
                        after.trim().to_string()
                    }
                } else {
                    text.trim().to_string()
                };
                info.summary = summary;
            }
        }

        // Fetch catalog page for chapter list
        let prefix = Self::chapter_prefix(book_id);
        let catalog_url = format!(
            "{}/read/{}/{}/index.html",
            self.base_url(),
            prefix,
            book_id
        );
        let catalog_html = client.get(&catalog_url).await?;
        let catalog_doc = Html::parse_document(&catalog_html);

        // Parse volumes and chapters
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        // Look for zjbox (volume headers) and zjlist4 (chapter lists)
        if let Ok(sel) = Selector::parse("div.zjbox, div.zjlist4") {
            for elem in catalog_doc.select(&sel) {
                let class = elem.value().attr("class").unwrap_or("");
                if class.contains("zjbox") {
                    // Volume header
                    if !current_chapters.is_empty() {
                        volumes.push(Volume {
                            volume_name: current_volume_name.clone(),
                            chapters: std::mem::take(&mut current_chapters),
                        });
                    }
                    current_volume_name = element_text(&elem);
                } else if class.contains("zjlist4") {
                    // Chapter list
                    if let Ok(a_sel) = Selector::parse("ol li a[href], a[href]") {
                        for a_elem in elem.select(&a_sel) {
                            if let Some(href) = a_elem.value().attr("href") {
                                let title = element_text(&a_elem);
                                if title.is_empty() {
                                    continue;
                                }
                                let chapter_id = href
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

        // Flush remaining
        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_volume_name,
                chapters: current_chapters,
            });
        }

        // Fallback: direct link extraction
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("a[href]") {
                for elem in catalog_doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        if !href.ends_with(".html") || href.contains("index") {
                            continue;
                        }
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
                volumes.push(Volume {
                    volume_name: String::new(),
                    chapters,
                });
            }
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
        let prefix = Self::chapter_prefix(book_id);
        let url = format!(
            "{}/read/{}/{}/{}.html",
            self.base_url(),
            prefix,
            book_id,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let mut title = select_text(&doc, "h1");
        // Remove book name prefix if present
        if !info_book_name_empty(&title) {
            // Just use h1 text as-is
        }

        // Content: extract text between BookSee_Right div and <!--over--> comment
        // Since we can't easily parse HTML comments with scraper, use regex fallback
        let mut content = String::new();

        // Try to find content div
        if let Ok(sel) = Selector::parse("#BookSee_Right, #BookText, #content") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
            }
        }

        // Fallback: extract text between markers using regex
        if content.is_empty() {
            if let Some(start) = html_str.find("id=\"BookSee_Right\"") {
                let after = &html_str[start..];
                let end = after.find("<!--over-->").unwrap_or(after.len());
                let segment = &after[..end];
                content = html_to_text(segment);
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

fn info_book_name_empty(_title: &str) -> bool {
    false
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(ShencouProvider)
}
