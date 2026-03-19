use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;
use crate::providers::biquge_common::select_attr_in;

pub struct PiaotiaProvider;

impl PiaotiaProvider {
    const BASE_URL: &'static str = "https://www.piaotia.com";
    const SEARCH_URL: &'static str = "https://www.piaotia.com/modules/article/search.php";
}

#[async_trait]
impl Provider for PiaotiaProvider {
    fn name(&self) -> &str {
        "piaotia"
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
            .post_form_with_encoding(
                Self::SEARCH_URL,
                &[
                    ("searchtype", "articlename"),
                    ("searchkey", keyword),
                    ("Submit", " 搜 索 "),
                ],
                encoding_rs::GBK,
            )
            .await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        let sel = Selector::parse("table.grid tr").unwrap();
        let td_a_sel = Selector::parse("td:first-child a").unwrap();

        for elem in doc.select(&sel) {
            let href = select_attr_in(&elem, "td:first-child a", "href");
            if href.is_empty() {
                continue;
            }

            // "https://www.piaotia.com/bookinfo/14/14767.html" -> "14-14767"
            let book_id = href.trim_end_matches(".html")
                .rsplit("bookinfo/").next().unwrap_or("")
                .replace('/', "-");
            if book_id.is_empty() {
                continue;
            }

            let title = elem.select(&td_a_sel).next()
                .map(|e| element_text(&e)).unwrap_or_default();

            let td2 = Selector::parse("td:nth-child(2) a").unwrap();
            let td3 = Selector::parse("td:nth-child(3)").unwrap();
            let td4 = Selector::parse("td:nth-child(4)").unwrap();
            let td5 = Selector::parse("td:nth-child(5)").unwrap();

            let latest_chapter = elem.select(&td2).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let author = elem.select(&td3).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let word_count = elem.select(&td4).next()
                .map(|e| element_text(&e)).unwrap_or_default();
            let update_date = elem.select(&td5).next()
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

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let real_id = book_id.replace('-', "/");
        let info_url = format!("{}/bookinfo/{}.html", Self::BASE_URL, real_id);
        let catalog_url = format!("{}/html/{}/index.html", Self::BASE_URL, real_id);

        let info_html = client.get_with_encoding(&info_url, encoding_rs::GBK).await?;
        let catalog_html = client.get_with_encoding(&catalog_url, encoding_rs::GBK).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "span[style] h1");

        info.author = extract_td_field(&info_doc, &["作", "者"])
            .replace("作者：", "").replace('\u{00a0}', "").replace(' ', "")
            .trim().to_string();

        info.word_count = extract_td_field(&info_doc, &["全文长度"])
            .replace("全文长度：", "").replace('\u{00a0}', "").replace(' ', "")
            .trim().to_string();

        info.update_time = extract_td_field(&info_doc, &["最后更新"])
            .replace("最后更新：", "").replace('\u{00a0}', "").replace(' ', "")
            .trim().to_string();

        info.serial_status = extract_td_field(&info_doc, &["文章状态"])
            .replace("文章状态：", "").replace('\u{00a0}', "").replace(' ', "")
            .trim().to_string();

        info.cover_url = select_attr(&info_doc, "td[width=\"80%\"] img", "src");

        let summary_text = select_text(&info_doc, "td[width=\"80%\"] div");
        info.summary = if summary_text.contains("内容简介：") {
            summary_text.split("内容简介：").last().unwrap_or("").trim().to_string()
        } else {
            summary_text
        };

        // Parse chapters from catalog
        let mut chapters = Vec::new();
        let sel = Selector::parse("div.centent ul li a").unwrap();
        for elem in catalog_doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title = elem.text().next().unwrap_or("").trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let chapter_id = href.split('.').next().unwrap_or("").to_string();
                chapters.push(ChapterInfo {
                    title,
                    chapter_id,
                    url: normalize_url(
                        &format!("{}/html/{}", Self::BASE_URL, real_id),
                        href,
                    ),
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
        let real_id = book_id.replace('-', "/");
        let url = format!("{}/html/{}/{}.html", Self::BASE_URL, real_id, chapter_id);
        let raw_html = client.get_with_encoding(&url, encoding_rs::GBK).await?;

        // Normalize broken HTML
        let normalized = raw_html
            .replace("<head>", "")
            .replace("</head>", "")
            .replace("<body>", "")
            .replace("</body>", "")
            .replace(
                "<script language=\"javascript\">GetMode();</script>",
                "<div id=\"main\" class=\"colors1 sidebar\">",
            )
            .replace(
                "<script language=\"javascript\">GetFont();</script>",
                "<div id=\"content\">",
            );

        let doc = Html::parse_document(&normalized);

        let mut title = String::new();
        let mut paragraphs = Vec::new();

        let sel = Selector::parse("div#content").unwrap();
        if let Some(content_elem) = doc.select(&sel).next() {
            for child in content_elem.children() {
                if let Some(elem_ref) = scraper::ElementRef::wrap(child) {
                    let tag = elem_ref.value().name();
                    if tag == "h1" && title.is_empty() {
                        title = element_text(&elem_ref);
                        continue;
                    }
                    if tag == "div" {
                        if let Some(cls) = elem_ref.value().attr("class") {
                            if cls.contains("toplink") {
                                continue;
                            }
                        }
                    }
                    if tag == "table" || tag == "script" || tag == "style" {
                        continue;
                    }
                    let txt = element_text(&elem_ref);
                    if !txt.is_empty() {
                        paragraphs.push(txt);
                    }
                }
                if let Some(text) = child.value().as_text() {
                    let txt = text.trim();
                    if !txt.is_empty() {
                        paragraphs.push(txt.to_string());
                    }
                }
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

fn extract_td_field(doc: &Html, keywords: &[&str]) -> String {
    let sel = Selector::parse("td").unwrap();
    for elem in doc.select(&sel) {
        let text = element_text(&elem);
        if keywords.iter().all(|kw| text.contains(kw)) {
            return text;
        }
    }
    String::new()
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(PiaotiaProvider)
}
