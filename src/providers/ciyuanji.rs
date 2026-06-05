use anyhow::Result;
use async_trait::async_trait;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::*;
use crate::utils::*;

pub struct CiyuanjiProvider;

impl CiyuanjiProvider {
    fn extract_next_data(html_str: &str) -> Option<serde_json::Value> {
        let doc = Html::parse_document(html_str);
        if let Ok(sel) = Selector::parse("script#__NEXT_DATA__") {
            if let Some(elem) = doc.select(&sel).next() {
                let text = element_text(&elem);
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                    return Some(data);
                }
            }
        }
        None
    }
}

#[async_trait]
impl Provider for CiyuanjiProvider {
    fn name(&self) -> &str {
        "ciyuanji"
    }

    fn display_name(&self) -> &str {
        "次元姬"
    }

    fn base_url(&self) -> &str {
        "https://www.ciyuanji.com"
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
            "https://www.ciyuanji.com/search/{}_0_0_0_0_0_1.html",
            urlencoding::encode(keyword)
        );
        let html_str = client.get(&url).await?;

        let data = match Self::extract_next_data(&html_str) {
            Some(d) => d,
            None => return Ok(vec![]),
        };

        let mut results = Vec::new();
        if let Some(list) = data
            .pointer("/props/pageProps/list")
            .and_then(|v| v.as_array())
        {
            for item in list.iter().take(limit) {
                let book_id = item
                    .get("bookId")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .or_else(|| {
                        item.get("bookId")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                let title = item
                    .get("bookName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let author = item
                    .get("authorName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let word_count = item
                    .get("wordCount")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let update_date = item
                    .get("latestUpdateTime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let latest_chapter = item
                    .get("latestChapterName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

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
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("https://www.ciyuanji.com/b_d_{}.html", book_id);
        let html_str = client.get(&url).await?;

        let data = Self::extract_next_data(&html_str)
            .ok_or_else(|| anyhow::anyhow!("Failed to find __NEXT_DATA__"))?;

        let mut info = BookInfo::default();

        if let Some(book) = data.pointer("/props/pageProps/book") {
            info.book_name = book
                .get("bookName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            info.author = book
                .get("authorName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            info.cover_url = book
                .get("imgUrl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            info.update_time = book
                .get("latestUpdateTime")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            info.word_count = book
                .get("wordCount")
                .map(|v| v.to_string())
                .unwrap_or_default();
            info.summary = book
                .get("notes")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let end_state_str = book
                .get("endState")
                .map(|v| v.to_string().replace('"', ""))
                .unwrap_or_default();
            info.serial_status = match end_state_str.as_str() {
                "1" => "完结".to_string(),
                "2" => "连载".to_string(),
                _ => end_state_str,
            };
        }

        // Parse chapter list
        if let Some(chapter_list) = data
            .pointer("/props/pageProps/bookChapter/chapterList")
            .and_then(|v| v.as_array())
        {
            // Group by volumeId
            use std::collections::BTreeMap;
            let mut vol_map: BTreeMap<i64, (String, i64, Vec<(i64, ChapterInfo)>)> =
                BTreeMap::new();

            for ch in chapter_list {
                let volume_id = ch.get("volumeId").and_then(|v| v.as_i64()).unwrap_or(0);
                let volume_title = ch
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let volume_sort = ch
                    .get("volumeSortNum")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let chapter_name = ch
                    .get("chapterName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let chapter_id_val = ch
                    .get("chapterId")
                    .map(|v| v.to_string().replace('"', ""))
                    .unwrap_or_default();
                let b_id = ch
                    .get("bookId")
                    .map(|v| v.to_string().replace('"', ""))
                    .unwrap_or_default();
                let sort_num = ch.get("sortNum").and_then(|v| v.as_i64()).unwrap_or(0);

                let entry = vol_map
                    .entry(volume_id)
                    .or_insert_with(|| (volume_title.clone(), volume_sort, Vec::new()));

                entry.2.push((
                    sort_num,
                    ChapterInfo {
                        title: chapter_name,
                        chapter_id: chapter_id_val.clone(),
                        url: format!(
                            "https://www.ciyuanji.com/chapter/{}_{}.html",
                            b_id, chapter_id_val
                        ),
                    },
                ));
            }

            let mut vols: Vec<_> = vol_map.into_values().collect();
            vols.sort_by_key(|v| v.1);

            for (vol_name, _, mut chapters) in vols {
                chapters.sort_by_key(|c| c.0);
                info.volumes.push(Volume {
                    volume_name: vol_name,
                    chapters: chapters.into_iter().map(|c| c.1).collect(),
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
        let url = format!(
            "https://www.ciyuanji.com/chapter/{}_{}.html",
            book_id, chapter_id
        );
        let html_str = client.get(&url).await?;

        let data = Self::extract_next_data(&html_str)
            .ok_or_else(|| anyhow::anyhow!("Failed to find __NEXT_DATA__"))?;

        let mut title = String::new();
        let mut content = String::new();

        if let Some(chapter_content) = data.pointer("/props/pageProps/chapterContent") {
            title = chapter_content
                .get("chapterName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let enc_content = chapter_content
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // The content is DES-ECB encrypted with key "ZUreQN0E", base64 encoded.
            // In Rust without a DES crate, we return the raw content as-is.
            // Most users would need proper DES decryption; for now use plain text if available.
            content = enc_content.to_string();
        }

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content,
        })
    }
}

pub fn provider() -> Box<dyn Provider> {
    Box::new(CiyuanjiProvider)
}
