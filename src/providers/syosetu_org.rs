use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

/// ハーメルン (syosetu.org) provider
pub struct SyosetuOrgProvider;

#[async_trait]
impl Provider for SyosetuOrgProvider {
    fn name(&self) -> &str {
        "syosetu_org"
    }

    fn display_name(&self) -> &str {
        "ハーメルン"
    }

    fn base_url(&self) -> &str {
        "https://syosetu.org"
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://syosetu.org/novel/{}/", book_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        let mut info = BookInfo::default();

        // Metadata from div.ss spans
        info.book_name = select_text(&doc, "div.ss span[itemprop=\"name\"]");
        info.author = select_text(&doc, "div.ss span[itemprop=\"author\"] a");

        // Summary from second div.ss
        let mut summary_parts = Vec::new();
        if let Ok(sel) = Selector::parse("div.ss") {
            let divs: Vec<_> = doc.select(&sel).collect();
            if divs.len() >= 2 {
                let text = element_text(&divs[1]);
                if !text.is_empty() {
                    summary_parts.push(text);
                }
            }
        }
        info.summary = summary_parts.join("\n");

        // Parse chapters from div.ss table
        let mut volumes: Vec<Volume> = Vec::new();
        let mut vol_idx = 1;
        let mut current_vol_name: Option<String> = None;
        let mut current_chapters: Vec<ChapterInfo> = Vec::new();

        if let Ok(table_sel) = Selector::parse("div.ss table") {
            if let Ok(tr_sel) = Selector::parse("tr") {
                for table in doc.select(&table_sel) {
                    for tr in table.select(&tr_sel) {
                        // Check for volume title (strong tag)
                        if let Ok(strong_sel) = Selector::parse("strong") {
                            if let Some(strong) = tr.select(&strong_sel).next() {
                                // Flush current volume
                                if !current_chapters.is_empty() {
                                    volumes.push(Volume {
                                        volume_name: current_vol_name
                                            .take()
                                            .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                                        chapters: std::mem::take(&mut current_chapters),
                                    });
                                    vol_idx += 1;
                                }
                                current_vol_name = Some(element_text(&strong));
                                continue;
                            }
                        }

                        // Check for chapter link
                        if let Ok(a_sel) = Selector::parse("a[href*=\".html\"]") {
                            if let Some(a) = tr.select(&a_sel).next() {
                                let href = a.value().attr("href").unwrap_or("").trim();
                                let title =
                                    a.text().collect::<Vec<_>>().join("").trim().to_string();
                                if !href.is_empty() {
                                    let chap_id = href
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or("")
                                        .split('.')
                                        .next()
                                        .unwrap_or("")
                                        .to_string();
                                    current_chapters.push(ChapterInfo {
                                        title,
                                        chapter_id: chap_id,
                                        url: normalize_url(self.base_url(), href),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Flush last volume
        if !current_chapters.is_empty() {
            volumes.push(Volume {
                volume_name: current_vol_name
                    .take()
                    .unwrap_or_else(|| format!("未命名卷 {}", vol_idx)),
                chapters: current_chapters,
            });
        }

        info.volumes = volumes;
        Ok(info)
    }

    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter> {
        let url = format!("https://syosetu.org/novel/{}/{}.html", book_id, chapter_id);
        let html_text = client.get(&url).await?;
        let doc = Html::parse_document(&html_text);

        // Title from large font span
        let title = select_text(&doc, "span[style*=\"font-size\"][style*=\"120%\"]");

        // Three-part content: preface, main, postscript
        let mut parts = Vec::new();

        let maegaki = select_text(&doc, "#maegaki");
        if !maegaki.is_empty() {
            parts.push(maegaki);
        }

        if let Ok(sel) = Selector::parse("#honbun p") {
            for p in doc.select(&sel) {
                let text = element_text(&p);
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }

        let atogaki = select_text(&doc, "#atogaki");
        if !atogaki.is_empty() {
            parts.push(atogaki);
        }

        let content = parts.join("\n");

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(SyosetuOrgProvider)
}
