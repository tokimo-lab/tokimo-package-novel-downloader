use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct LaoyaoxsProvider;

impl LaoyaoxsProvider {
    const BASE_URL: &'static str = "https://www.laoyaoxs.org";
    const SEARCH_URL: &'static str = "https://www.laoyaoxs.org/search.php";
}

#[async_trait]
impl Provider for LaoyaoxsProvider {
    fn name(&self) -> &str {
        "laoyaoxs"
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
        let url = format!("{}?key={}", Self::SEARCH_URL, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("div.result li").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let href = {
                let h = select_attr_in(&elem, "a.book_cov", "href");
                if h.is_empty() {
                    select_attr_in(&elem, "div.book_inf h3 a", "href")
                } else {
                    h
                }
            };
            if href.is_empty() {
                continue;
            }

            let book_id = href
                .rsplit('/')
                .next()
                .unwrap_or("")
                .split('.')
                .next()
                .unwrap_or("")
                .to_string();

            let mut cover_url = select_attr_in(&elem, "a.book_cov img", "data-original");
            if cover_url.is_empty() {
                cover_url = select_attr_in(&elem, "a.book_cov img", "src");
            }
            if cover_url.starts_with("//") {
                cover_url = format!("https:{}", cover_url);
            }

            let title = select_attr_in(&elem, "div.book_inf h3 a", "title");
            let author = select_text_in(&elem, "p.tags span a");
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

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let info_url = format!("{}/info/{}.html", Self::BASE_URL, book_id);
        let catalog_url = format!("{}/list/{}/", Self::BASE_URL, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = meta_content(&info_doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = select_text(&info_doc, "div.detail h1");
        }

        info.author = meta_content(&info_doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&info_doc, "div.detail p a");
        }

        info.cover_url = meta_content(&info_doc, "og:image");
        if info.cover_url.is_empty() {
            info.cover_url = select_attr(&info_doc, "a.bookimg img", "src");
        }
        if info.cover_url.starts_with("//") {
            info.cover_url = format!("https:{}", info.cover_url);
        }

        info.update_time = meta_content(&info_doc, "og:novel:update_time")
            .replace('\u{00a0}', " ")
            .replace('\u{3000}', " ");

        info.summary = select_text(&info_doc, "p.intro");

        // Parse chapters from catalog page
        let mut chapters = Vec::new();
        let sel = Selector::parse("div.read dl#newlist dd a:first-child").unwrap();
        for elem in catalog_doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title = elem
                    .value()
                    .attr("title")
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| element_text(&elem));
                if title.is_empty() {
                    continue;
                }
                let chapter_id = href.split('.').next().unwrap_or("").to_string();
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
        let url = format!("{}/list/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = {
            let t = select_text(&doc, "div#chapter-name h2");
            if t.is_empty() {
                select_text(&doc, "h2")
            } else {
                t
            }
        };

        // Parse content from dd[data-id] elements, sorted by data-id
        let mut fragments: Vec<(i64, String)> = Vec::new();
        let sel = Selector::parse("div.main_content dd[data-id], div#txt dd[data-id]").unwrap();
        let p_sel = Selector::parse("p").unwrap();
        for elem in doc.select(&sel) {
            let order_idx: i64 = elem
                .value()
                .attr("data-id")
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(-1);
            if order_idx < 0 {
                continue;
            }

            let mut dd_text = String::new();
            for p in elem.select(&p_sel) {
                let txt = element_text(&p);
                if !txt.is_empty() {
                    if !dd_text.is_empty() {
                        dd_text.push('\n');
                    }
                    dd_text.push_str(&txt);
                }
            }
            if !dd_text.is_empty() {
                fragments.push((order_idx, dd_text));
            }
        }

        fragments.sort_by_key(|(idx, _)| *idx);
        let content = clean_content(
            &fragments
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(LaoyaoxsProvider)
}
