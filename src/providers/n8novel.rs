use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::providers::biquge_common::select_text_in;
use crate::types::*;
use crate::utils::*;

pub struct N8novelProvider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(N8novelProvider)
}

const BASE_URL: &str = "https://www.8novel.com";

/// Check if a line matches the obfuscated "8novel.com" ad pattern.
fn is_n8novel_ad(line: &str) -> bool {
    const AD_SETS: &[&[char]] = &[
        &['8', '⑧', '⑻', '⒏', '８'],
        &['N', 'Ν', 'Ｎ', 'ｎ'],
        &['O', 'o', 'ο', 'σ', 'О', 'Ｏ', 'ｏ'],
        &['v', 'ν', 'Ｖ'],
        &['E', 'Ε', 'Ё', 'Е', 'ヨ', 'Ｅ', 'ｅ'],
        &['L', '└', '┕', '┗', 'Ｌ', 'ｌ'],
        &['.', '·', '。', '．'],
        &['C', 'c', 'С', 'с', 'Ｃ', 'ｃ'],
        &['o', 'Ο', 'ο', 'О', 'о', 'Ｏ'],
        &['m', 'м', 'ｍ'],
    ];

    let chars: Vec<char> = line.chars().collect();
    if chars.len() != AD_SETS.len() {
        return false;
    }
    let mut mismatches = 0;
    for (i, ch) in chars.iter().enumerate() {
        if !AD_SETS[i].contains(ch) {
            mismatches += 1;
            if mismatches > 2 {
                return false;
            }
        }
    }
    true
}

#[async_trait]
impl Provider for N8novelProvider {
    fn name(&self) -> &str {
        "n8novel"
    }

    fn display_name(&self) -> &str {
        "无限轻小说"
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
        let url = format!("{}/search/?key={}", BASE_URL, urlencoding::encode(keyword));
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut results = Vec::new();

        if let Ok(sel) = Selector::parse("div.picsize a[href]") {
            for elem in doc.select(&sel).take(limit) {
                let href = elem.value().attr("href").unwrap_or("");
                if href.is_empty() {
                    continue;
                }
                // '/novelbooks/6045' -> "6045"
                let book_id = href
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let title = elem.value().attr("title").unwrap_or("").to_string();

                results.push(SearchResult {
                    site: self.name().to_string(),
                    book_id,
                    title,
                    author: String::new(),
                    latest_chapter: String::new(),
                    update_date: String::new(),
                    word_count: String::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo> {
        let url = format!("{}/novelbooks/{}/", BASE_URL, book_id);
        let html = client.get(&url).await?;
        let doc = Html::parse_document(&html);

        let mut info = BookInfo::default();
        info.book_name = select_text(&doc, "li.h2");
        info.author = select_text(&doc, "span.item-info-author")
            .replace("作者: ", "")
            .replace("作者：", "")
            .trim()
            .to_string();
        info.cover_url = format!(
            "{}{}",
            BASE_URL,
            select_attr(&doc, "div.item-cover img", "src")
        );
        info.update_time = select_text(&doc, "span.item-info-date")
            .replace("更新: ", "")
            .replace("更新：", "")
            .trim()
            .to_string();

        // Word count from second item-info-num span
        if let Ok(sel) = Selector::parse("li.small.text-gray span.item-info-num") {
            let spans: Vec<_> = doc.select(&sel).collect();
            if spans.len() >= 2 {
                let count = element_text(&spans[1]);
                info.word_count = format!("{}萬字", count.trim());
            }
        }

        // Summary
        info.summary = select_text(&doc, "li.full_text.mt-2");

        // Volumes & Chapters
        if let Ok(vol_sel) = Selector::parse("div.folder[pid]") {
            for vol_div in doc.select(&vol_sel) {
                let vol_name = select_text_in(&vol_div, "div.vol-title h3")
                    .split('/')
                    .next()
                    .unwrap_or("Unnamed Volume")
                    .trim()
                    .to_string();

                let mut chapters = Vec::new();
                if let Ok(a_sel) = Selector::parse("a.episode_li.d-block[href]") {
                    for a in vol_div.select(&a_sel) {
                        let title = element_text(&a);
                        let href = a.value().attr("href").unwrap_or("");
                        if title.is_empty() || href.is_empty() {
                            continue;
                        }
                        let full_url = if href.starts_with("http") {
                            href.to_string()
                        } else {
                            format!("{}{}", BASE_URL, href)
                        };
                        // "/read/3355/?270015" -> "270015"
                        let chapter_id = href.split('?').last().unwrap_or("").to_string();
                        chapters.push(ChapterInfo {
                            title,
                            chapter_id,
                            url: full_url,
                        });
                    }
                }

                info.volumes.push(Volume {
                    volume_name: vol_name,
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
        // Step 1: Fetch the chapter page to get txt_dir and url_seed
        let chapter_page_url = format!(
            "https://article.8novel.com/read/{}/?{}",
            book_id, chapter_id
        );
        let chapter_html = client.get(&chapter_page_url).await?;

        // Extract txt_dir from %2f(\d)% pattern
        let txt_dir_re = Regex::new(r"%2f(\d)%")?;
        let txt_dir = txt_dir_re
            .captures(&chapter_html)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "1".to_string());

        // Extract URL seed from split pattern
        let split_re =
            Regex::new(r#"["\'](\d+(?:,\d+)*)["\']\.split\s*\(\s*["\']\s*,\s*["\']\s*\)"#)?;
        let url_seed = {
            let mut seed = String::new();
            for caps in split_re.captures_iter(&chapter_html) {
                seed = caps[1].to_string();
            }
            if seed.is_empty() {
                "00000".to_string()
            } else {
                seed.split(',').last().unwrap_or("00000").to_string()
            }
        };

        // Build seed segment
        let chap_num: usize = chapter_id.parse().unwrap_or(0);
        let start = (chap_num * 3) % 100;
        let seed_segment: String = url_seed.chars().skip(start).take(5).collect();

        // Step 2: Fetch actual content
        let content_url = format!(
            "https://article.8novel.com/txt/{}/{}/{}{}.html",
            txt_dir, book_id, chapter_id, seed_segment
        );
        let content_html = client.get(&content_url).await?;

        // Try to extract title from the chapter page JS
        let title = {
            let split_str_re =
                Regex::new(r#"["\']([^"\']+)["\']\.split\s*\(\s*["\']\s*,\s*["\']\s*\)"#)?;
            let mut id_list: Option<Vec<String>> = None;
            let mut title_list: Option<Vec<String>> = None;
            for caps in split_str_re.captures_iter(&chapter_html) {
                let content = caps[1].to_string();
                let items: Vec<String> = content.split(',').map(|s| s.trim().to_string()).collect();
                if items
                    .iter()
                    .all(|s| s.is_empty() || s.chars().all(|c| c.is_ascii_digit()))
                {
                    if items.len() > 1 && !items.iter().all(|s| s.is_empty()) {
                        id_list = Some(items);
                    }
                } else if title_list.is_none() {
                    title_list = Some(items);
                }
                if id_list.is_some() && title_list.is_some() {
                    break;
                }
            }
            if let (Some(ids), Some(titles)) = (&id_list, &title_list) {
                let pos = ids.iter().position(|id| id == chapter_id);
                pos.and_then(|p| titles.get(p).cloned()).unwrap_or_default()
            } else {
                String::new()
            }
        };

        // Parse content HTML
        let wrapped = format!("<div>{}</div>", content_html);
        let content = html_to_text(&wrapped);
        // Filter out ad lines
        let filtered: Vec<&str> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !is_n8novel_ad(trimmed)
            })
            .collect();

        Ok(Chapter {
            id: chapter_id.to_string(),
            title,
            content: filtered.join("\n"),
        })
    }
}
