use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

// ============================================================
// Biquge1: og:* meta tags + book_list2 divs + article content
// Used by: biquge5, biquguo, bxwx9, ciluke, fsshu, ktshu, n37yue, mangg_net
// ============================================================

pub struct Biquge1Provider {
    pub name: &'static str,
    pub base_url: &'static str,
    pub search_url: &'static str,
    pub use_paginated_info: bool,
    pub use_paginated_chapter: bool,
}

impl Biquge1Provider {
    fn info_url(&self, book_id: &str, page: usize) -> String {
        if page > 1 {
            format!("{}/{}/index_{}.html", self.base_url, book_id, page)
        } else {
            format!("{}/{}/", self.base_url, book_id)
        }
    }

    fn chapter_url(&self, book_id: &str, chapter_id: &str, page: usize) -> String {
        if page > 1 {
            format!("{}/{}/{}_{}.html", self.base_url, book_id, chapter_id, page)
        } else {
            format!("{}/{}/{}.html", self.base_url, book_id, chapter_id)
        }
    }

    fn parse_book_info_html(&self, html: &str) -> Result<BookInfo> {
        let doc = Html::parse_document(html);
        let mut info = BookInfo::default();

        // Try og:novel:book_name or og:title
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }

        info.author = meta_content(&doc, "og:novel:author");
        if info.author.is_empty() {
            info.author = select_text(&doc, ".info span, .book_info a, #info h1+p a");
        }

        info.summary = meta_content(&doc, "og:description");
        info.cover_url = meta_content(&doc, "og:image");

        // Parse chapter list from book_list2 divs or dl structure
        let mut chapters = Vec::new();

        // Try book_list2 pattern first
        if let Ok(sel) = Selector::parse(
            "div.book_list2 a[href], div.listmain a[href], #list a[href], .zjlist a[href]",
        ) {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
                        .next()
                        .unwrap_or(href)
                        .to_string();
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url, href),
                    });
                }
            }
        }

        if !chapters.is_empty() {
            info.volumes.push(Volume {
                volume_name: String::new(),
                chapters,
            });
        }

        Ok(info)
    }

    fn parse_chapter_html(&self, html: &str) -> Result<Chapter> {
        let doc = Html::parse_document(html);

        let title = select_text(&doc, "h1");

        // Try article > content extraction
        let content_selectors = [
            "article",
            "#content",
            "#booktext",
            ".content",
            "#chaptercontent",
            ".chapter-content",
            "#TextContent",
            ".read-content",
            "#booktxt",
        ];

        let mut content = String::new();
        for sel_str in &content_selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                if let Some(elem) = doc.select(&sel).next() {
                    content = elem.inner_html();
                    break;
                }
            }
        }

        let text = html_to_text(&content);

        Ok(Chapter {
            id: String::new(),
            title,
            content: text,
        })
    }
}

#[async_trait]
impl Provider for Biquge1Provider {
    fn name(&self) -> &str {
        self.name
    }

    fn base_url(&self) -> &str {
        self.base_url
    }

    fn supports_search(&self) -> bool {
        !self.search_url.is_empty()
    }

    async fn search(
        &self,
        client: &HttpClient,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        crate::providers::mangg_search::mangg_search(
            client,
            self.search_url,
            self.base_url,
            self.name,
            keyword,
            limit,
        )
        .await
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = self.info_url(book_id, 1);
        let html = client.get(&url).await?;
        let mut info = self.parse_book_info_html(&html)?;

        // Handle pagination
        if self.use_paginated_info {
            let mut page = 2;
            loop {
                let next_url = self.info_url(book_id, page);
                let relative = format!("index_{}.html", page);
                if !html.contains(&relative) {
                    break;
                }
                if let Ok(page_html) = client.get(&next_url).await {
                    let page_info = self.parse_book_info_html(&page_html)?;
                    if let Some(vol) = page_info.volumes.first() {
                        if let Some(main_vol) = info.volumes.first_mut() {
                            main_vol.chapters.extend(vol.chapters.clone());
                        }
                    }
                }
                page += 1;
                if page > 50 {
                    break;
                }
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
        let url = self.chapter_url(book_id, chapter_id, 1);
        let html = client.get(&url).await?;
        let mut chapter = self.parse_chapter_html(&html)?;
        chapter.id = chapter_id.to_string();

        // Handle paginated chapters
        if self.use_paginated_chapter {
            let mut page = 2;
            loop {
                let next_url = self.chapter_url(book_id, chapter_id, page);
                let relative = format!("{}_{}.html", chapter_id, page);
                if !html.contains(&relative) {
                    break;
                }
                if let Ok(page_html) = client.get(&next_url).await {
                    let page_chapter = self.parse_chapter_html(&page_html)?;
                    chapter.content.push('\n');
                    chapter.content.push_str(&page_chapter.content);
                }
                page += 1;
                if page > 20 {
                    break;
                }
            }
        }

        Ok(chapter)
    }
}

// ============================================================
// Biquge2: div#info + div#content + dl/dt/dd chapter list
// Used by: blqudu, lewenn
// ============================================================

pub struct Biquge2Provider {
    pub name: &'static str,
    pub base_url: &'static str,
}

#[async_trait]
impl Provider for Biquge2Provider {
    fn name(&self) -> &str {
        self.name
    }

    fn base_url(&self) -> &str {
        self.base_url
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", self.base_url, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "#info h1");
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }
        info.author = select_text(&doc, "#info p:first-of-type");
        info.author = info
            .author
            .replace("作    者：", "")
            .replace("作者：", "")
            .trim()
            .to_string();
        info.summary = select_text(&doc, "#intro");
        info.cover_url = select_attr(&doc, "#fmimg img", "src");

        // Parse chapters from dd a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("dd a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
                        .next()
                        .unwrap_or(href)
                        .to_string();
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url, href),
                    });
                }
            }
        }

        info.volumes.push(Volume {
            volume_name: String::new(),
            chapters,
        });

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        // chapter_id should contain the full path or we construct it
        let url = if chapter_id.starts_with("http") {
            chapter_id.to_string()
        } else {
            format!("{}/{}", self.base_url, chapter_id)
        };
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1");
        let mut content = String::new();
        if let Ok(sel) = Selector::parse("#content") {
            if let Some(elem) = doc.select(&sel).next() {
                content = html_to_text(&elem.inner_html());
            }
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

// ============================================================
// Biquge3: og:* meta + div#list/dl chapter list
// Used by: n23ddw, n69hao
// ============================================================

pub struct Biquge3Provider {
    pub name: &'static str,
    pub base_url: &'static str,
    pub search_url: &'static str,
}

#[async_trait]
impl Provider for Biquge3Provider {
    fn name(&self) -> &str {
        self.name
    }

    fn base_url(&self) -> &str {
        self.base_url
    }

    fn supports_search(&self) -> bool {
        !self.search_url.is_empty()
    }

    async fn search(
        &self,
        client: &HttpClient,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let url = format!("{}{}", self.search_url, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) =
            Selector::parse(".result-list .result-item, .search-list li, .novelslist2 li")
        {
            for elem in doc.select(&sel).take(limit) {
                let title = select_text_in(&elem, "a, .s2 a, h3 a");
                let author = select_text_in(&elem, ".author, .s4, span.s4");
                let href = select_attr_in(&elem, "a[href], .s2 a[href], h3 a[href]", "href");
                if title.is_empty() {
                    continue;
                }
                let book_id = extract_book_id(&href);
                results.push(SearchResult {
                    site: self.name.to_string(),
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
        let url = format!("{}/{}/", self.base_url, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = meta_content(&doc, "og:novel:book_name");
        if info.book_name.is_empty() {
            info.book_name = meta_content(&doc, "og:title");
        }
        if info.book_name.is_empty() {
            info.book_name = select_text(&doc, "h1");
        }
        info.author = meta_content(&doc, "og:novel:author");
        info.summary = meta_content(&doc, "og:description");
        info.cover_url = meta_content(&doc, "og:image");

        // Parse chapters from #list a or dl a
        let mut chapters = Vec::new();
        if let Ok(sel) = Selector::parse("#list a[href], .zjlist a[href], dl dd a[href]") {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
                        .next()
                        .unwrap_or(href)
                        .to_string();
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url, href),
                    });
                }
            }
        }

        info.volumes.push(Volume {
            volume_name: String::new(),
            chapters,
        });

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = if chapter_id.starts_with("http") {
            chapter_id.to_string()
        } else {
            format!("{}/{}", self.base_url, chapter_id)
        };
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1");
        let mut content = String::new();

        for sel_str in &["#booktxt", "#content", "#chaptercontent", "article"] {
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

// ============================================================
// Biquge4: Regex-based (malformed HTML mobile sites)
// Used by: mjyhb, shauthor
// ============================================================

pub struct Biquge4Provider {
    pub name: &'static str,
    pub base_url: &'static str,
}

#[async_trait]
impl Provider for Biquge4Provider {
    fn name(&self) -> &str {
        self.name
    }

    fn base_url(&self) -> &str {
        self.base_url
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/{}/", self.base_url, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "h1, .title, .bookname");
        info.author = select_text(&doc, ".bq_intro span, .author, .info span");

        let mut chapters = Vec::new();
        if let Ok(sel) =
            Selector::parse("ul li a[href], .chapterlist a[href], #chapterlist a[href]")
        {
            for elem in doc.select(&sel) {
                if let Some(href) = elem.value().attr("href") {
                    let title = element_text(&elem);
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = href
                        .trim_end_matches(".html")
                        .rsplit('/')
                        .next()
                        .unwrap_or(href)
                        .to_string();
                    chapters.push(ChapterInfo {
                        title,
                        chapter_id,
                        url: normalize_url(self.base_url, href),
                    });
                }
            }
        }

        info.volumes.push(Volume {
            volume_name: String::new(),
            chapters,
        });

        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        _book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = if chapter_id.starts_with("http") {
            chapter_id.to_string()
        } else {
            format!("{}/{}", self.base_url, chapter_id)
        };
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let title = select_text(&doc, "h1, .title");

        // Try regex-based content extraction for malformed HTML
        let content = if let Some(cap) =
            regex::Regex::new(r#"id="novelcontent"[^>]*>([\s\S]*?)</div>"#)
                .ok()
                .and_then(|re| re.captures(&html))
        {
            html_to_text(&cap[1])
        } else {
            let mut c = String::new();
            for sel_str in &[
                "#novelcontent",
                "#content",
                "#chaptercontent",
                "article",
                ".content",
            ] {
                if let Ok(sel) = Selector::parse(sel_str) {
                    if let Some(elem) = doc.select(&sel).next() {
                        c = html_to_text(&elem.inner_html());
                        if !c.is_empty() {
                            break;
                        }
                    }
                }
            }
            c
        };

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

// ============================================================
// Helper functions used by providers
// ============================================================

pub fn select_text_in(elem: &scraper::ElementRef, selector_str: &str) -> String {
    if let Ok(sel) = Selector::parse(selector_str) {
        if let Some(child) = elem.select(&sel).next() {
            return element_text(&child);
        }
    }
    String::new()
}

pub fn select_attr_in(elem: &scraper::ElementRef, selector_str: &str, attr: &str) -> String {
    if let Ok(sel) = Selector::parse(selector_str) {
        if let Some(child) = elem.select(&sel).next() {
            if let Some(val) = child.value().attr(attr) {
                return val.to_string();
            }
        }
    }
    String::new()
}

pub fn extract_book_id(href: &str) -> String {
    let path = href
        .trim_end_matches('/')
        .trim_end_matches(".html")
        .trim_end_matches(".htm");
    path.rsplit('/').next().unwrap_or(href).to_string()
}
