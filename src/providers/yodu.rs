use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct YoduProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(YoduProvider)
}

const BASE_URL: &str = "https://www.yodu.org";

#[async_trait]
impl Provider for YoduProvider {
    fn name(&self) -> &str {
        "yodu"
    }

    fn display_name(&self) -> &str {
        "有度中文网"
    }

    fn base_url(&self) -> &str {
        BASE_URL
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
        let html = client
            .post_form(
                "https://www.yodu.org/sa",
                &[("searchkey", keyword), ("searchtype", "all")],
            )
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("ul.ser-ret li") {
            for elem in doc.select(&sel).take(limit) {
                let href = {
                    let v = select_attr_in(&elem, "a.g_thumb[href]", "href");
                    if v.is_empty() {
                        select_attr_in(&elem, "h3 a[href]", "href")
                    } else {
                        v
                    }
                };
                if href.is_empty() {
                    continue;
                }
                // '/book/17551/?for-search' -> "17551"
                let book_id = if href.contains("/book/") {
                    href.split("/book/")
                        .last()
                        .unwrap_or("")
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .to_string()
                } else {
                    href.trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string()
                };

                let title = {
                    let v = select_attr_in(&elem, "h3 a[title]", "title");
                    if v.is_empty() {
                        select_text_in(&elem, "h3 a")
                    } else {
                        v
                    }
                };
                let author = select_text_in(&elem, "em span:nth-child(2)");
                let latest_chapter = select_text_in(&elem, "p a");

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter,
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/book/{}/", BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();

        // Metadata from og:* meta tags with fallbacks
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "div.det-info h1");
        }

        info.author = meta_content(&doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&doc, "div.det-info p._tags strong a");
        }

        info.serial_status = meta_content(&doc, "og:novel:status");
        info.update_time = meta_content(&doc, "og:novel:update_time");
        if info.update_time.is_empty() {
            info.update_time = select_text(&doc, "#Contents p small");
        }

        info.cover_url = meta_content(&doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&doc, "div.det-info div.cover img", "src");
        }

        info.summary = meta_content(&doc, "og:description")
            .replace('\r', "")
            .replace("\n\n", "\n");
        if info.summary.is_empty() {
            info.summary = select_text(&doc, "div.det-info div.det-abt p");
        }

        // Volumes & Chapters from #chapterList
        let mut volumes: Vec<Volume> = Vec::new();
        let mut current_vol_name: Option<String> = None;
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        let flush = |volumes: &mut Vec<Volume>,
                     vol_name: &mut Option<String>,
                     chapters: &mut Vec<ChapterInfo>| {
            if chapters.is_empty() {
                return;
            }
            volumes.push(Volume {
                volume_name: vol_name.take().unwrap_or_else(|| "正文".to_string()),
                chapters: std::mem::take(chapters),
            });
        };

        if let Ok(sel) = Selector::parse("ol#chapterList > li") {
            for li in doc.select(&sel) {
                let li_class = li.value().attr("class").unwrap_or("");

                if li_class.contains("volumes") {
                    flush(&mut volumes, &mut current_vol_name, &mut current_chapters);
                    current_vol_name = Some(element_text(&li));
                    continue;
                }

                if let Ok(a_sel) = Selector::parse("a[href]") {
                    if let Some(a) = li.select(&a_sel).next() {
                        if let Some(href) = a.value().attr("href") {
                            let title = element_text(&a);
                            if href.contains("javascript") || href.is_empty() {
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
                            current_chapters.push(ChapterInfo {
                                title,
                                chapter_id,
                                url: normalize_url(BASE_URL, href),
                            });
                        }
                    }
                }
            }
        }

        flush(&mut volumes, &mut current_vol_name, &mut current_chapters);
        info.volumes = volumes;

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/book/{}/{}.html", BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;

        // Extract title and paragraphs in a scope so doc is dropped before any further awaits
        let (title, paragraphs) = {
            let doc = Html::parse_document(&html);
            let title = select_text(&doc, "#mlfy_main_text h1");
            let mut paragraphs = Vec::new();
            if let Ok(sel) = Selector::parse("#TextContent p") {
                for elem in doc.select(&sel) {
                    let text = element_text(&elem);
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                }
            }
            (title, paragraphs)
        };

        // Handle paginated chapters
        let mut full_content = paragraphs.join("\n");
        let page_re = regex::Regex::new(r#"nextpage="/book/\d+/(\d+)\.html""#).ok();
        if let Some(re) = &page_re {
            let mut current_html = html.clone();
            let mut page = 2;
            while re.is_match(&current_html) {
                let next_url =
                    format!("{}/book/{}/{}_{}.html", BASE_URL, book_id, chapter_id, page);
                match client.get(&next_url).await {
                    Ok(next_html) => {
                        let next_doc = Html::parse_document(&next_html);
                        let mut next_paras = Vec::new();
                        if let Ok(sel) = Selector::parse("#TextContent p") {
                            for elem in next_doc.select(&sel) {
                                let text = element_text(&elem);
                                if !text.is_empty() {
                                    next_paras.push(text);
                                }
                            }
                        }
                        if !next_paras.is_empty() {
                            full_content.push('\n');
                            full_content.push_str(&next_paras.join("\n"));
                        }
                        current_html = next_html;
                    }
                    Err(_) => break,
                }
                page += 1;
                if page > 20 {
                    break;
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: full_content,
        })
    }
}
