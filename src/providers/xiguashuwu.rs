use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct XiguashuwuProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(XiguashuwuProvider)
}

const BASE_URL: &str = "https://www.xiguashuwu.com";

#[async_trait]
impl Provider for XiguashuwuProvider {
    fn name(&self) -> &str {
        "xiguashuwu"
    }

    fn display_name(&self) -> &str {
        "西瓜书屋"
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
        let url = format!("{}/search/{}", BASE_URL, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.SHsectionThree-middle p") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "a[href]", "href");
                if href.is_empty() {
                    continue;
                }
                // '/book/184974/iszip/0/' -> "184974"
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
                let title = select_text_in(&elem, "a");
                // Try to get author from second link (writer link)
                let author = {
                    if let Ok(a_sel) = Selector::parse("a[href*='/writer/']") {
                        if let Some(a) = elem.select(&a_sel).next() {
                            element_text(&a)
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                };

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // Fetch info page
        let info_url = format!("{}/book/{}/iszip/0/", BASE_URL, book_id);
        let info_html = client.get(&info_url).await?;

        // Extract all info from doc before any further awaits (Html is not Send)
        let mut info = {
            let info_doc = Html::parse_document(&info_html);
            let mut info = BookInfo::default();
            info.book_name = select_text(&info_doc, "p.title");
            info.author = select_text(&info_doc, "p.author a");
            let cover_rel = {
                let v = select_attr(&info_doc, "div.BGsectionOne-top-left img", "_src");
                if v.is_empty() {
                    select_attr(&info_doc, "div.BGsectionOne-top-left img", "src")
                } else {
                    v
                }
            };
            info.cover_url = format!("{}{}", BASE_URL, cover_rel);
            info.update_time = select_text(&info_doc, "p.time span");
            let paras: Vec<String> = if let Ok(sel) = Selector::parse("section#intro p") {
                info_doc
                    .select(&sel)
                    .map(|e| element_text(&e))
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                vec![]
            };
            info.summary = paras.join("\n");
            info
        };

        // Fetch catalog pages
        let mut chapters = Vec::new();
        let catalog_url = format!("{}/book/{}/catalog/", BASE_URL, book_id);
        let catalog_html = client.get(&catalog_url).await?;
        self.parse_catalog_chapters(&catalog_html, &mut chapters);

        // Handle paginated catalog
        let mut page = 2;
        loop {
            let next_patterns = [
                format!("javascript:readbookjump('{}','{}');", book_id, page),
                format!("javascript:gobookjump('{}','{}');", book_id, page),
                format!("javascript:runbookjump('{}','{}');", book_id, page),
                format!("javascript:gotojump('{}','{}');", book_id, page),
                format!("javascript:gotochapterjump('{}','{}');", book_id, page),
                format!("/book/{}/catalog/{}.html", book_id, page),
            ];
            if !next_patterns.iter().any(|p| catalog_html.contains(p)) {
                break;
            }
            let next_url = format!("{}/book/{}/catalog/{}.html", BASE_URL, book_id, page);
            if let Ok(next_html) = client.get(&next_url).await {
                self.parse_catalog_chapters(&next_html, &mut chapters);
            }
            page += 1;
            if page > 100 {
                break;
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
        let url = format!("{}/book/{}/{}.html", BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1#chapterTitle");

        // Extract text from #C0NTENT (note the zero instead of O)
        let mut content = String::new();
        for sel_str in &["#C0NTENT", "#content", "div.content"] {
            if let Ok(sel) = Selector::parse(sel_str) {
                if let Some(elem) = doc.select(&sel).next() {
                    content = html_to_text(&elem.inner_html());
                    if !content.is_empty() {
                        break;
                    }
                }
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

impl XiguashuwuProvider {
    fn parse_catalog_chapters(&self, html: &str, chapters: &mut Vec<ChapterInfo>) {
        let doc = Html::parse_document(html);
        // Look for chapters under 正文 section
        if let Ok(sel) = Selector::parse("section.BCsectionTwo ol li a[href]") {
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
                        url: format!("{}{}", BASE_URL, href),
                    });
                }
            }
        }
    }
}
