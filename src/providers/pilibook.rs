use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct PilibookProvider;

impl PilibookProvider {
    fn normalize_book_id(book_id: &str) -> String {
        book_id.replace('-', "/")
    }
}

#[async_trait]
impl Provider for PilibookProvider {
    fn name(&self) -> &str {
        "pilibook"
    }

    fn display_name(&self) -> &str {
        "霹雳书屋"
    }

    fn base_url(&self) -> &str {
        "https://www.pilibook.net"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let normalized = Self::normalize_book_id(book_id);

        // Fetch info page and extract all data before next await
        let info_url = format!("{}/{}/info.html", self.base_url(), normalized);
        let html_str = client.get(&info_url).await?;
        let mut info = BookInfo::default();
        {
            let doc = Html::parse_document(&html_str);

            info.book_name = select_text(
                &doc,
                "h2.works-intro-title strong, h2[class*='works-intro-title'] strong",
            );
            if info.book_name.is_empty() {
                info.book_name = select_text(&doc, "h2.works-intro-title, h1");
            }

            info.author = select_text(&doc, "a.works-author-name, a[class*='works-author-name']");
            info.serial_status = select_text(
                &doc,
                "label.works-intro-status, label[class*='works-intro-status']",
            );

            let mut cover = select_attr(
                &doc,
                "div.works-cover img, div[class*='works-cover'] img",
                "src",
            );
            if cover.starts_with("//") {
                cover = format!("https:{}", cover);
            } else if !cover.is_empty() && !cover.starts_with("http") {
                cover = format!("{}{}", self.base_url(), cover);
            }
            info.cover_url = cover;

            info.summary = select_text(&doc, "p.works-intro-short, p[class*='works-intro-short']");

            if let Ok(sel) =
                Selector::parse("ul.works-chapter-log li, ul[class*='works-chapter-log'] li")
            {
                for elem in doc.select(&sel) {
                    let text = element_text(&elem);
                    if text.contains("最新章") {
                        if let Ok(span_sel) =
                            Selector::parse("span.ui-text-gray6, span[class*='ui-text-gray6']")
                        {
                            if let Some(span) = elem.select(&span_sel).next() {
                                info.update_time = element_text(&span);
                            }
                        }
                        break;
                    }
                }
            }
        }

        // Fetch catalog page for chapter list
        let catalog_url = format!("{}/{}/menu/1.html", self.base_url(), normalized);
        let catalog_html = client.get(&catalog_url).await?;
        let catalog_doc = Html::parse_document(&catalog_html);

        // Parse volumes and chapters
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_volume_name = String::new();
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        if let Ok(sel) = Selector::parse(
            "div.works-chapter-list-wr > *, div[class*='works-chapter-list-wr'] > *",
        ) {
            for elem in catalog_doc.select(&sel) {
                let class = elem.value().attr("class").unwrap_or("");

                if class.contains("vloume") || class.contains("volume") {
                    // Volume header
                    if !current_chapters.is_empty() {
                        volumes.push(Volume {
                            volume_name: current_volume_name.clone(),
                            chapters: std::mem::take(&mut current_chapters),
                        });
                    }
                    current_volume_name = element_text(&elem);
                } else if class.contains("chapter-page-new") || elem.value().name() == "ol" {
                    // Chapter list
                    if let Ok(a_sel) = Selector::parse("a[href]") {
                        for a_elem in elem.select(&a_sel) {
                            if let Some(href) = a_elem.value().attr("href") {
                                let title = element_text(&a_elem);
                                if title.is_empty() {
                                    continue;
                                }
                                // Extract chapter ID from /read/{chapter_id}.html
                                let chapter_id = href
                                    .rsplit("/read/")
                                    .next()
                                    .unwrap_or("")
                                    .trim_end_matches(".html")
                                    .to_string();
                                if chapter_id.is_empty() {
                                    // Fallback extraction
                                    let cid = href
                                        .trim_end_matches(".html")
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or("")
                                        .to_string();
                                    current_chapters.push(ChapterInfo {
                                        title,
                                        chapter_id: cid,
                                        url: normalize_url(self.base_url(), href),
                                    });
                                } else {
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
        }

        // Flush remaining
        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_volume_name,
                chapters: current_chapters,
            });
        }

        // Fallback: simple chapter list
        if volumes.is_empty() {
            let mut chapters = Vec::new();
            if let Ok(sel) = Selector::parse("ol a[href], li a[href]") {
                for elem in catalog_doc.select(&sel) {
                    if let Some(href) = elem.value().attr("href") {
                        if !href.contains("/read/") {
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
        let normalized = Self::normalize_book_id(book_id);
        let url = format!(
            "{}/{}/read/{}.html",
            self.base_url(),
            normalized,
            chapter_id
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        // Title from j_chapterName span
        let mut title = select_text(
            &doc,
            "h3.j_chapterName span, h3[class*='j_chapterName'] span",
        );
        if title.is_empty() {
            title = select_text(&doc, "h3.j_chapterName, h1");
        }

        // Content from j_readContent p
        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.j_readContent p, div[class*='j_readContent'] p") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
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
    Box::new(PilibookProvider)
}
