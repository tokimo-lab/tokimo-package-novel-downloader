use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct ShuhaigeProvider;

impl ShuhaigeProvider {
    const BASE_URL: &'static str = "https://www.shuhaige.net";
    const SEARCH_URL: &'static str = "https://www.shuhaige.net/search.html";
}

fn is_ad_line(text: &str) -> bool {
    let patterns = ["www.shuhaige.net", "书海阁小说网", "点击下一页"];
    patterns.iter().any(|p| text.contains(p))
}

#[async_trait]
impl Provider for ShuhaigeProvider {
    fn name(&self) -> &str {
        "shuhaige"
    }

    fn base_url(&self) -> &str {
        Self::BASE_URL
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
                Self::SEARCH_URL,
                &[("searchtype", "all"), ("searchkey", keyword)],
            )
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("div#sitembox dl").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let href = {
                let h = select_attr_in(&elem, "dt a", "href");
                if h.is_empty() {
                    select_attr_in(&elem, "dd h3 a", "href")
                } else {
                    h
                }
            };
            if href.is_empty() {
                continue;
            }

            let book_id = href
                .trim_matches('/')
                .split('/')
                .next()
                .unwrap_or("")
                .to_string();

            let title = {
                let t = select_text_in(&elem, "dd h3 a");
                if t.is_empty() {
                    select_attr_in(&elem, "dt a img", "alt")
                } else {
                    t
                }
            };

            let cover_rel = select_attr_in(&elem, "dt a img", "src");
            let _cover_url = if !cover_rel.is_empty() {
                normalize_url(Self::BASE_URL, &cover_rel)
            } else {
                String::new()
            };

            let author = select_text_in(&elem, "dd.book_other:first-of-type span:first-child");
            let word_count = select_text_in(&elem, "dd.book_other:first-of-type span:nth-child(4)");
            let latest_chapter = select_text_in(&elem, "dd.book_other:last-of-type a");
            let update_date = select_text_in(&elem, "dd.book_other:last-of-type span:first-child");

            results.push(SearchResult {
                site: self.name().to_string(),
                book_id,
                title,
                author,
                latest_chapter,
                update_date,
                word_count,
            });
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", Self::BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&doc, "div#info h1");
        info.author = select_text(&doc, "div#info p:first-of-type a");
        info.cover_url = select_attr(&doc, "div#fmimg img", "src");
        info.update_time = select_text(&doc, "div#info p:nth-of-type(3)")
            .replace("最后更新：", "")
            .trim()
            .to_string();
        info.summary = select_text(&doc, "div#intro p:first-of-type");

        // Parse chapters: after dt containing "正文"
        let mut chapters = Vec::new();
        let mut found_zhengwen = false;

        let sel = Selector::parse("div#list dl > dt, div#list dl > dd").unwrap();
        let a_sel = Selector::parse("a").unwrap();

        for elem in doc.select(&sel) {
            let tag = elem.value().name();
            if tag == "dt" {
                let text = element_text(&elem);
                if text.contains("正文") {
                    found_zhengwen = true;
                }
                continue;
            }
            if tag == "dd" && found_zhengwen {
                if let Some(a) = elem.select(&a_sel).next() {
                    let href = a.value().attr("href").unwrap_or("");
                    let title = a.text().next().unwrap_or("").trim().to_string();
                    if title.is_empty() || href.is_empty() {
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
                        url: normalize_url(Self::BASE_URL, href),
                    });
                }
            }
        }

        // Fallback: if no "正文" dt found, get all dd a
        if chapters.is_empty() {
            let sel = Selector::parse("div#list dl dd a").unwrap();
            for a in doc.select(&sel) {
                let href = a.value().attr("href").unwrap_or("");
                let title = a.text().next().unwrap_or("").trim().to_string();
                if title.is_empty() || href.is_empty() {
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
                    url: normalize_url(Self::BASE_URL, href),
                });
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
        let mut title = String::new();
        let mut all_paragraphs: Vec<String> = Vec::new();
        let mut page = 1;

        loop {
            let url = if page == 1 {
                format!("{}/{}/{}.html", Self::BASE_URL, book_id, chapter_id)
            } else {
                format!(
                    "{}/{}/{}_{}.html",
                    Self::BASE_URL,
                    book_id,
                    chapter_id,
                    page
                )
            };

            let html = client.get(&url).await?;
            let doc = Html::parse_document(&html);

            if title.is_empty() {
                title = select_text(&doc, "div.bookname h1");
            }

            let sel = Selector::parse("div#content p").unwrap();
            for elem in doc.select(&sel) {
                let txt = element_text(&elem);
                if !txt.is_empty() && !is_ad_line(&txt) {
                    all_paragraphs.push(txt);
                }
            }

            // Check for next page
            let next_page = page + 1;
            let next_suffix = format!("{}_{}.html", chapter_id, next_page);
            if html.contains(&next_suffix) {
                page = next_page;
            } else {
                break;
            }

            if page > 20 {
                break;
            }
        }

        let content = clean_content(&all_paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(ShuhaigeProvider)
}
