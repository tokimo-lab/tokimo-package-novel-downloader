use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 百合会 (yamibo.com) provider
pub struct YamiboProvider;

#[async_trait]
impl Provider for YamiboProvider {
    fn name(&self) -> &str {
        "yamibo"
    }

    fn display_name(&self) -> &str {
        "百合会"
    }

    fn base_url(&self) -> &str {
        "https://www.yamibo.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.yamibo.com/novel/{}", book_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "h3.col-md-12");
        info.author = select_text(&doc, "h5.text-warning");

        let cover_path = select_attr(&doc, "img.img-responsive", "src");
        if !cover_path.is_empty() {
            info.cover_url = format!("{}{}", self.base_url(), cover_path);
        }

        // Extract metadata from p elements
        if let Ok(p_sel) = Selector::parse("p") {
            for p in doc.select(&p_sel) {
                let text = element_text(&p);
                if text.contains("更新时间：") {
                    info.update_time = text.replace("更新时间：", "").trim().to_string();
                } else if text.contains("作品状态：") {
                    info.serial_status = text.replace("作品状态：", "").trim().to_string();
                }
            }
        }

        // Summary from collapse div
        info.summary = select_text(&doc, "#w0-collapse1 div");

        // Parse volumes from panel-info panel-default divs
        let mut volumes: Vec<Volume> = Vec::new();

        if let Ok(vol_sel) = Selector::parse("div.panel-info.panel-default") {
            for vol_node in doc.select(&vol_sel) {
                let vol_name = {
                    let name = if let Ok(heading_sel) = Selector::parse("div.panel-heading a") {
                        vol_node
                            .select(&heading_sel)
                            .next()
                            .map(|e| element_text(&e))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if name.is_empty() {
                        "未命名卷".to_string()
                    } else {
                        name
                    }
                };

                let mut chapters = Vec::new();
                if let Ok(chap_sel) =
                    Selector::parse("div.panel-body a[href*=\"view-chapter\"]")
                {
                    for chap in vol_node.select(&chap_sel) {
                        let title = element_text(&chap);
                        let href = chap.value().attr("href").unwrap_or("");
                        // Extract chapter_id from query param ?id=XXX
                        let chapter_id = href
                            .split("id=")
                            .last()
                            .unwrap_or("")
                            .to_string();
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id,
                            url: normalize_url(self.base_url(), href),
                        });
                    }
                }

                if !chapters.is_empty() {
                    volumes.push(Volume {
                        volume_name: vol_name,
                        chapters,
                    });
                }
            }
        }

        // Fallback: flat chapter list
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(chap_sel) =
                Selector::parse("div.panel-body a[href*=\"view-chapter\"]")
            {
                for chap in doc.select(&chap_sel) {
                    let title = element_text(&chap);
                    let href = chap.value().attr("href").unwrap_or("");
                    let chapter_id = if href.contains("id=") {
                        href.split("id=").last().unwrap_or("").to_string()
                    } else {
                        String::new()
                    };
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url(), href),
                    });
                }
            }
            if !chapters.is_empty() {
                volumes.push(Volume {
                    volume_name: "单卷".to_string(),
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
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!(
            "https://www.yamibo.com/novel/view-chapter?id={}",
            chapter_id
        );
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "section.col-md-9 h3");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#w0-collapse1 p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                let normalized = text
                    .replace('\u{00a0}', " ")
                    .replace('\u{3000}', "  ");
                let trimmed = normalized.trim().to_string();
                if !trimmed.is_empty() {
                    paragraphs.push(trimmed);
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
    Box::new(YamiboProvider)
}
