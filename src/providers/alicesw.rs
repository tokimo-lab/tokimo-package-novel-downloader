use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::{select_attr_in, select_text_in};
use crate::types::*;
use crate::utils::*;

pub struct AliceswProvider;

#[async_trait]
impl Provider for AliceswProvider {
    fn name(&self) -> &str {
        "alicesw"
    }

    fn display_name(&self) -> &str {
        "爱丽丝书屋"
    }

    fn base_url(&self) -> &str {
        "https://www.alicesw.com"
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
            "{}?q={}&f=_all",
            "https://www.alicesw.com/search.html",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.list-group-item") {
            for elem in doc.select(&sel).take(limit) {
                let href = select_attr_in(&elem, "h5 a", "href");
                if href.is_empty() {
                    continue;
                }

                let book_id = href
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let title = select_text_in(&elem, "h5 a");
                let author = select_text_in(&elem, "p.text-muted a");
                let update_date_raw = select_text_in(&elem, "p.timedesc");
                let update_date = update_date_raw
                    .split("更新时间：")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: String::new(),
                    update_date,
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        // book_id uses "-" as separator, convert to "/" for URLs
        let book_id_path = book_id.replace('-', "/");

        let info_url = format!("https://www.alicesw.com/novel/{}.html", book_id_path);
        let catalog_url = format!(
            "https://www.alicesw.com/other/chapters/id/{}.html",
            book_id_path
        );

        let info_html = client.get(&info_url).await?;
        let catalog_html = client.get(&catalog_url).await?;

        let info_doc = Html::parse_document(&info_html);
        let catalog_doc = Html::parse_document(&catalog_html);

        let mut info = BookInfo::default();

        info.book_name = select_text(&info_doc, "#detail-box h1");
        info.author = select_text(&info_doc, "#detail-box p a[href*='author']");

        info.cover_url = select_attr(&info_doc, "div.pic img.fengmian2", "src");

        info.summary = select_text(&info_doc, "div.intro");

        // Parse chapters from catalog page
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("ul.mulu_list li a[href]") {
            for elem in catalog_doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .split(".html")
                        .next()
                        .unwrap_or(href)
                        .split("book/")
                        .last()
                        .unwrap_or("")
                        .replace('/', "-");
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url(), href),
                    });
                }
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
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let chapter_id_path = chapter_id.replace('-', "/");
        let url = format!("https://www.alicesw.com/book/{}.html", chapter_id_path);
        let html_str = client.get(&url).await?;
        let doc = Html::parse_document(&html_str);

        let title = select_text(&doc, "h3.j_chapterName");

        let mut content = String::new();
        if let Ok(sel) = Selector::parse("div.read-content p") {
            let paragraphs: Vec<String> = doc
                .select(&sel)
                .map(|p| element_text(&p))
                .filter(|t| !t.is_empty())
                .collect();
            content = paragraphs.join("\n");
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(AliceswProvider)
}
