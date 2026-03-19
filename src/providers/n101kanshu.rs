use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

pub struct N101kanshuProvider;

impl N101kanshuProvider {
    const BASE_URL: &'static str = "https://101kanshu.com";
    const SEARCH_URL: &'static str = "https://101kanshu.com/search";
}

#[async_trait]
impl Provider for N101kanshuProvider {
    fn name(&self) -> &str {
        "n101kanshu"
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
            .post_form(Self::SEARCH_URL, &[
                ("searchkey", keyword),
                ("searchtype", "all"),
            ])
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("ul#article_list_content li").unwrap();
        for elem in doc.select(&sel).take(limit) {
            let book_url = select_attr_in(&elem, "a.imgbox", "href");
            if book_url.is_empty() {
                continue;
            }

            let book_id = book_url.trim_end_matches('/')
                .rsplit('/').next().unwrap_or("")
                .split('.').next().unwrap_or("").to_string();

            let mut cover_url = select_attr_in(&elem, "img", "data-src");
            if cover_url.is_empty() {
                cover_url = select_attr_in(&elem, "img", "src");
            }
            if !cover_url.is_empty() {
                cover_url = normalize_url(Self::BASE_URL, &cover_url);
            }

            let title = select_text_in(&elem, "div.newnav h3 a");
            let author = select_text_in(&elem, "div.labelbox label:first-child");
            let latest_chapter = select_text_in(&elem, "div.zxzj a");

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
        let info_url = format!("{}/book/{}.html", Self::BASE_URL, book_id);
        let catalog_url = format!("{}/ajax_novels/chapterlist/{}.html", Self::BASE_URL, book_id);

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = meta_content(&info_doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&info_doc, "og:title");
        }
        info.author = meta_content(&info_doc, "og:novel:author");
        info.cover_url = meta_content(&info_doc, "og:image");
        info.serial_status = meta_content(&info_doc, "og:novel:status");
        info.update_time = meta_content(&info_doc, "og:novel:update_time");

        let raw_summary = meta_content(&info_doc, "og:description");
        info.summary = raw_summary.replace("<br />", "\n");

        // Word count
        if let Ok(p_sel) = Selector::parse("p") {
            for elem in info_doc.select(&p_sel) {
                let text = element_text(&elem);
                if text.contains('字') {
                    info.word_count = text.split('|').next().unwrap_or("").trim().to_string();
                    break;
                }
            }
        }

        // Parse chapters from catalog
        let mut chapters = Vec::new();
        let sel = Selector::parse("ul a[href]").unwrap();
        for elem in catalog_doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title = element_text(&elem);
                if href.is_empty() {
                    continue;
                }
                let chapter_id = href.rsplit('/').next().unwrap_or("")
                    .split('.').next().unwrap_or("").to_string();
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
        let url = format!("{}/txt/{}/{}.html", Self::BASE_URL, book_id, chapter_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "div.txtnav h1");

        let mut paragraphs = Vec::new();
        let sel = Selector::parse("div#txtcontent").unwrap();
        if let Some(content_elem) = doc.select(&sel).next() {
            extract_text_recursive(&content_elem, &mut paragraphs);
        }

        let content = clean_content(&paragraphs.join("\n"));

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

fn extract_text_recursive(elem: &scraper::ElementRef, parts: &mut Vec<String>) {
    use scraper::Node;
    for child in elem.children() {
        match child.value() {
            Node::Text(text) => {
                let txt = text.trim();
                if !txt.is_empty() {
                    parts.push(txt.to_string());
                }
            }
            Node::Element(el) => {
                let tag = el.name();
                if tag == "script" || tag == "style" {
                    continue;
                }
                if tag == "div" {
                    if let Some(cls) = el.attr("class") {
                        if cls.contains("txtad") {
                            continue;
                        }
                    }
                }
                if let Some(child_ref) = scraper::ElementRef::wrap(child) {
                    extract_text_recursive(&child_ref, parts);
                }
            }
            _ => {}
        }
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(N101kanshuProvider)
}
