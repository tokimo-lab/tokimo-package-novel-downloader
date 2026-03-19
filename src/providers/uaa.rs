use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct UaaProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(UaaProvider)
}

#[async_trait]
impl Provider for UaaProvider {
    fn name(&self) -> &str {
        "uaa"
    }

    fn display_name(&self) -> &str {
        "有爱爱"
    }

    fn base_url(&self) -> &str {
        "https://www.uaa.com"
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
            "https://www.uaa.com/novel/list?searchType=1&keyword={}",
            urlencoding::encode(keyword)
        );
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("li.novel_li_2") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "div.cover_box a[href]", "href");
                if href.is_empty() {
                    continue;
                }
                let book_id = if href.contains("id=") {
                    href.split("id=").last().unwrap_or("").to_string()
                } else {
                    href.trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string()
                };
                let title = select_text_in(&elem, "div.title a");
                let author = select_text_in(&elem, "div.info_box a");
                let latest_chapter =
                    select_text_in(&elem, "div.update_state_box span.update_desc");
                let word_count = select_text_in(&elem, "div.other_box span");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date: String::new(),
                    word_count,
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.uaa.com/novel/intro?id={}", book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "div.info_box h1");
        info.cover_url = select_attr(&doc, "img.cover", "src");
        info.serial_status = select_text(&doc, "span.update_state")
            .replace("状态：", "");

        // Author: find <div class="item"> containing "作者" and get its <a>
        if let Ok(sel) = Selector::parse("div.item") {
            for item in doc.select(&sel) {
                let text = element_text(&item);
                if text.contains("作者") {
                    if let Ok(a_sel) = Selector::parse("a") {
                        if let Some(a) = item.select(&a_sel).next() {
                            info.author = element_text(&a);
                        }
                    }
                    break;
                }
            }
        }

        // Summary
        info.summary = select_text(&doc, "div.brief_box div.txt")
            .replace("小说简介：", "");

        // Volumes & Chapters
        let mut volumes: Vec<Volume> = Vec::new();
        let mut vol_idx = 1;
        let mut current_vol_name: Option<String> = None;
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        let flush = |volumes: &mut Vec<Volume>,
                     vol_idx: &mut usize,
                     vol_name: &mut Option<String>,
                     chapters: &mut Vec<ChapterInfo>| {
            if chapters.is_empty() {
                return;
            }
            volumes.push(Volume {
                volume_name: vol_name
                    .take()
                    .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                chapters: std::mem::take(chapters),
            });
            *vol_idx += 1;
        };

        if let Ok(sel) = Selector::parse("ul.catalog_ul > li") {
            for li in doc.select(&sel) {
                let li_class = li.value().attr("class").unwrap_or("");

                if li_class.contains("volume") {
                    flush(
                        &mut volumes,
                        &mut vol_idx,
                        &mut current_vol_name,
                        &mut current_chapters,
                    );
                    current_vol_name = Some(select_text_in(&li, "span"));

                    // Extract children chapters within volume
                    if let Ok(child_sel) = Selector::parse("ul.children li.child a[href]") {
                        for a in li.select(&child_sel) {
                            if let Some(href) = a.value().attr("href") {
                                let title = element_text(&a);
                                if href.is_empty() || title.is_empty() {
                                    continue;
                                }
                                let chapter_id = if href.contains("id=") {
                                    href.split("id=").last().unwrap_or("").to_string()
                                } else {
                                    href.rsplit('/')
                                        .next()
                                        .unwrap_or("")
                                        .to_string()
                                };
                                current_chapters.push(ChapterInfo {
                                    title,
                                    chapter_id,
                                    url: normalize_url(self.base_url(), href),
                                });
                            }
                        }
                    }

                    flush(
                        &mut volumes,
                        &mut vol_idx,
                        &mut current_vol_name,
                        &mut current_chapters,
                    );
                    continue;
                }

                if li_class.contains("menu") {
                    if let Ok(a_sel) = Selector::parse("a[href]") {
                        if let Some(a) = li.select(&a_sel).next() {
                            if let Some(href) = a.value().attr("href") {
                                let title = element_text(&a);
                                if !href.is_empty() && !title.is_empty() {
                                    let chapter_id = if href.contains("id=") {
                                        href.split("id=").last().unwrap_or("").to_string()
                                    } else {
                                        href.rsplit('/')
                                            .next()
                                            .unwrap_or("")
                                            .to_string()
                                    };
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

        flush(
            &mut volumes,
            &mut vol_idx,
            &mut current_vol_name,
            &mut current_chapters,
        );
        info.volumes = volumes;

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("https://www.uaa.com/novel/chapter?id={}", chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "div.title_box h2");

        let mut paragraphs = Vec::new();
        if let Ok(sel) = Selector::parse("div.article div.line") {
            for elem in doc.select(&sel) {
                let text = element_text(&elem);
                if !text.is_empty() {
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
