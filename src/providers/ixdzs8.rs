use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct Ixdzs8Provider;

impl Ixdzs8Provider {
    const BASE_URL: &'static str = "https://ixdzs8.com";
    const SEARCH_URL: &'static str = "https://ixdzs8.com/bsearch";
    const CATALOG_URL: &'static str = "https://ixdzs8.com/novel/clist/";

    async fn fetch_verified_html(&self, client: &HttpClient, url: &str) -> Result<String> {
        let html = client.get(url).await?;
        if !html.contains("正在验证浏览器") {
            return Ok(html);
        }
        let token_re = Regex::new(r#"let\s+token\s*=\s*"([^"]+)""#)?;
        if let Some(cap) = token_re.captures(&html) {
            let token = &cap[1];
            let challenge_url = format!("{}?challenge={}", url, token);
            let _ = client.get(&challenge_url).await;
            return client.get(url).await;
        }
        Ok(html)
    }
}

#[async_trait]
impl Provider for Ixdzs8Provider {
    fn name(&self) -> &str {
        "ixdzs8"
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
        let url = format!("{}?q={}", Self::SEARCH_URL, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("ul.u-list li.burl").unwrap();
        let bname_sel = Selector::parse("h3.bname a").unwrap();
        let bauthor_sel = Selector::parse("span.bauthor a").unwrap();
        let size_sel = Selector::parse("span.size").unwrap();
        let lchapter_sel = Selector::parse("p.l-last span.l-chapter").unwrap();
        let ltime_sel = Selector::parse("p.l-last span.l-time").unwrap();

        for elem in doc.select(&sel).take(limit) {
            let book_path = elem.value().attr("data-url")
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    elem.select(&bname_sel).next()
                        .and_then(|e| e.value().attr("href"))
                })
                .unwrap_or("");
            if book_path.is_empty() {
                continue;
            }

            let book_id = book_path.trim_end_matches('/').rsplit('/').next()
                .unwrap_or("").to_string();

            let title = elem.select(&bname_sel).next()
                .map(|e| e.value().attr("title").unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| elem.select(&bname_sel).next().map(|e| element_text(&e)))
                .unwrap_or_default();

            let author = elem.select(&bauthor_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let word_count = elem.select(&size_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let latest_chapter = elem.select(&lchapter_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let update_date = elem.select(&ltime_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();

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
        let url = format!("{}/read/{}/", Self::BASE_URL, book_id);
        let info_html = self.fetch_verified_html(client, &url).await?;

        let mut info = BookInfo::default();
        {
            let doc = Html::parse_document(&info_html);
            info.book_name = meta_content(&doc, "og:novel:book_name");
            if info.book_name.is_empty() {
                info.book_name = select_text(&doc, "div.n-text h1");
            }

            info.author = meta_content(&doc, "og:novel:author");
            info.cover_url = meta_content(&doc, "og:image");
            if info.cover_url.is_empty() {
                info.cover_url = select_attr(&doc, "div.n-img img", "src");
            }
            info.serial_status = meta_content(&doc, "og:novel:status");

            let iso_time = meta_content(&doc, "og:novel:update_time");
            if !iso_time.is_empty() {
                info.update_time = iso_time.replace('T', " ").split('+').next()
                    .unwrap_or("").trim().to_string();
            }

            info.word_count = select_text(&doc, "div.n-text span.nsize");

            let raw_summary = meta_content(&doc, "og:description");
            if !raw_summary.is_empty() {
                let s = raw_summary.replace("&nbsp;", "").replace("<br />", "\n");
                info.summary = s.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n").trim().to_string();
            }
        }

        // Fetch catalog via POST
        let catalog_resp = client.post_form(Self::CATALOG_URL, &[("bid", book_id)]).await?;
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&catalog_resp) {
            let mut chapters = Vec::new();
            if let Some(clist) = data.get("data").and_then(|d| d.as_array()) {
                for chap in clist {
                    let ordernum = chap.get("ordernum")
                        .and_then(|v| v.as_u64().map(|n| n.to_string())
                            .or_else(|| v.as_str().map(|s| s.to_string())))
                        .unwrap_or_default();
                    if ordernum.is_empty() {
                        continue;
                    }
                    let title = chap.get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("未命名章节")
                        .trim()
                        .to_string();
                    let chapter_id = format!("p{}", ordernum);
                    let chap_url = format!("/read/{}/{}.html", book_id, chapter_id);
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(Self::BASE_URL, &chap_url),
                    });
                }
            }
            if !chapters.is_empty() {
                info.volumes.push(Volume {
                    volume_name: "正文".to_string(),
                    chapters,
                });
            }
        }

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("{}/read/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html_str = self.fetch_verified_html(client, &url).await?;
        let doc = Html::parse_document(&html_str);

        let mut title = select_text(&doc, "div.page-d-top h1");
        if title.is_empty() {
            title = select_text(&doc, "article.page-content h3");
        }

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("article.page-content section p:not(.abg)").unwrap();
        for elem in doc.select(&sel) {
            let txt = element_text(&elem);
            if txt.is_empty() {
                continue;
            }
            paragraphs.push(txt);
        }

        // Remove title from first paragraph
        if !paragraphs.is_empty() {
            let first = paragraphs[0].replace(&title, "")
                .replace(&title.replace(' ', ""), "").trim().to_string();
            if first.is_empty() {
                paragraphs.remove(0);
            } else {
                paragraphs[0] = first;
            }
        }

        // Remove trailing "本章完"
        if let Some(last) = paragraphs.last() {
            if last.contains("本章完") {
                paragraphs.pop();
            }
        }

        let content = clean_content(&paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(Ixdzs8Provider)
}
