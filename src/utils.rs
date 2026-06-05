#![allow(dead_code)]

use once_cell::sync::Lazy;
use regex::Regex;

static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\s\u{00a0}\u{3000}]+").unwrap());
static AD_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"第\(\d+/\d+\)页").unwrap(),
        Regex::new(r"关闭小说畅读模式体验更好").unwrap(),
        Regex::new(r"内容未完，下一页继续阅读").unwrap(),
        Regex::new(r"(?i)www\.[a-z0-9]+\.(com|net|org|cc)").unwrap(),
        Regex::new(r"请记住本书首发域名").unwrap(),
        Regex::new(r"笔趣阁.*?手机版阅读网址").unwrap(),
        Regex::new(r"本章未完.*?点击下一页继续阅读").unwrap(),
        Regex::new(r"最新网址").unwrap(),
    ]
});

/// Strip HTML tags from text
pub fn strip_html(html: &str) -> String {
    HTML_TAG_RE.replace_all(html, "").to_string()
}

/// Clean up novel content text
pub fn clean_content(text: &str) -> String {
    let mut result = text.to_string();

    // Remove ad patterns
    for pattern in AD_PATTERNS.iter() {
        result = pattern.replace_all(&result, "").to_string();
    }

    // Normalize whitespace characters
    result = result.replace('\u{00a0}', " ").replace('\u{3000}', "  ");

    // Clean up excessive blank lines
    let lines: Vec<&str> = result.lines().collect();
    let mut cleaned_lines = Vec::new();
    let mut prev_empty = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_empty {
                cleaned_lines.push("");
                prev_empty = true;
            }
        } else {
            cleaned_lines.push(trimmed);
            prev_empty = false;
        }
    }

    cleaned_lines.join("\n")
}

/// Extract text content from HTML, preserving paragraph structure
pub fn html_to_text(html: &str) -> String {
    // Replace <br> and <p> with newlines
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n");

    let stripped = strip_html(&text);
    clean_content(&stripped)
}

/// Normalize a URL - make relative URLs absolute
pub fn normalize_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if url.starts_with("//") {
        return format!("https:{}", url);
    }
    let base = base.trim_end_matches('/');
    if url.starts_with('/') {
        // Extract scheme + host from base
        if let Some(idx) = base.find("//") {
            if let Some(host_end) = base[idx + 2..].find('/') {
                return format!("{}{}", &base[..idx + 2 + host_end], url);
            }
        }
        return format!("{}{}", base, url);
    }
    format!("{}/{}", base, url)
}

/// Extract text from a scraper Element, joining all text nodes
pub fn element_text(element: &scraper::ElementRef) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

/// Extract text from first matching selector
pub fn select_text(doc: &scraper::Html, selector_str: &str) -> String {
    if let Ok(selector) = scraper::Selector::parse(selector_str) {
        if let Some(elem) = doc.select(&selector).next() {
            return element_text(&elem);
        }
    }
    String::new()
}

/// Extract attribute from first matching selector
pub fn select_attr(doc: &scraper::Html, selector_str: &str, attr: &str) -> String {
    if let Ok(selector) = scraper::Selector::parse(selector_str) {
        if let Some(elem) = doc.select(&selector).next() {
            if let Some(val) = elem.value().attr(attr) {
                return val.to_string();
            }
        }
    }
    String::new()
}

/// Extract meta tag content by property or name
pub fn meta_content(doc: &scraper::Html, key: &str) -> String {
    // Try property first (og:title, etc.)
    let prop_sel = format!("meta[property=\"{}\"]", key);
    let val = select_attr(doc, &prop_sel, "content");
    if !val.is_empty() {
        return val;
    }
    // Try name
    let name_sel = format!("meta[name=\"{}\"]", key);
    select_attr(doc, &name_sel, "content")
}
