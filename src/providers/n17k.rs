use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// 17K小说网 provider (separate catalog, anti-bot cookie handling)
pub struct N17kProvider;

#[async_trait]
impl Provider for N17kProvider {
    fn name(&self) -> &str {
        "n17k"
    }

    fn display_name(&self) -> &str {
        "17K小说网"
    }

    fn base_url(&self) -> &str {
        "https://www.17k.com"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("https://www.17k.com/book/{}.html", book_id);
        let catalog_url = format!("https://www.17k.com/list/{}.html", book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        // From info page
        info.book_name = select_text(&info_doc, "div.Info.Sign h1 a");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "h1 a");
        }

        info.cover_url = select_attr(&info_doc, "#bookCover img", "src");
        info.serial_status = select_text(&info_doc, "div.label span");
        info.word_count = select_text(&info_doc, "div.BookData p em.red");

        // Update time
        info.update_time = select_text(&info_doc, "dl#bookInfo em")
            .replace("更新:", "")
            .trim()
            .to_string();

        info.summary = select_text(&info_doc, "p.intro");

        // Author from catalog page
        info.author = select_text(&catalog_doc, "div.Author a");

        // Volumes from catalog
        let mut volumes: Vec<Volume> = Vec::new();
        let mut vol_idx = 1;

        if let Ok(vol_sel) = Selector::parse("dl.Volume") {
            for vol in catalog_doc.select(&vol_sel) {
                let vol_name = {
                    if let Ok(tit_sel) = Selector::parse("dt span.tit") {
                        vol.select(&tit_sel)
                            .next()
                            .map(|e| element_text(&e))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                let mut chapters = Vec::new();
                if let Ok(a_sel) = Selector::parse("dd a[href]") {
                    for a in vol.select(&a_sel) {
                        let href = a.value().attr("href").unwrap_or("").trim();
                        if href.is_empty() {
                            continue;
                        }

                        // Get title from inner span
                        let title = if let Ok(span_sel) = Selector::parse("span") {
                            a.select(&span_sel)
                                .next()
                                .map(|s| element_text(&s))
                                .unwrap_or_default()
                        } else {
                            element_text(&a)
                        };

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
                            url: normalize_url(self.base_url(), href),
                        });
                    }
                }

                if !chapters.is_empty() {
                    volumes.push(Volume {
                        volume_name: if vol_name.is_empty() {
                            format!("未命名卷 {}", vol_idx)
                        } else {
                            vol_name
                        },
                        chapters,
                    });
                    vol_idx += 1;
                }
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
        let url = format!(
            "https://www.17k.com/chapter/{}/{}.html",
            book_id, chapter_id
        );
        let html_text = client.get(&url).await?;

        // Check for VIP chapter
        if html_text.contains("VIP章节, 余下还有") {
            return Ok(Chapter {
                id: chapter_id.to_string(),
                title: String::new(),
                content: "[VIP章节，需要购买后阅读]".to_string(),
            });
        }

        let doc = Html::parse_document(&html_text);

        let title = select_text(&doc, "#readArea h1");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("#readArea div.p p:not([class])") {
            for p in doc.select(&sel) {
                let text = p.text().collect::<Vec<_>>().join("").trim().to_string();
                if !text.is_empty() {
                    paragraphs.push(text);
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
    Box::new(N17kProvider)
}
