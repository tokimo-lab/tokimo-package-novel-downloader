use anyhow::Result;
use scraper::{Html, Selector};

use crate::client::HttpClient;
use crate::types::SearchResult;
use crate::utils::element_text;
use crate::providers::biquge_common::{select_text_in, select_attr_in};

/// Shared ManggNet-style search implementation
/// Used by: biquge5, biquguo, bxwx9, ciluke, fsshu, ktshu, n37yue, mangg_net
pub async fn mangg_search(
    client: &HttpClient,
    search_url: &str,
    _base_url: &str,
    site_name: &str,
    keyword: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let html = client
        .post_form(search_url, &[("q", keyword)])
        .await?;
    let doc = Html::parse_document(&html);
    let mut results = Vec::new();

    // ManggNet search results pattern: col-12 col-md-6 dl elements
    let selectors = [
        ".col-12.col-md-6 dl",
        ".search-result-list .result-item",
        ".novelslist2 li",
        ".result-list li",
        "table.grid tr",
        ".search-list li",
        ".list-group-item",
    ];

    for sel_str in &selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            let items: Vec<_> = doc.select(&sel).collect();
            if items.is_empty() {
                continue;
            }

            for elem in items.into_iter().take(limit) {
                // Try to extract title and link
                let title = select_text_in(&elem, "dt a, h3 a, a.s2, .s2 a, a:first-of-type");
                let href = select_attr_in(&elem, "dt a[href], h3 a[href], a.s2[href], .s2 a[href], a[href]:first-of-type", "href");
                let author = select_text_in(&elem, "dd span:nth-of-type(1), .s4, span.author, .author");
                let latest = select_text_in(&elem, "dd span:nth-of-type(2), .s3, span.update");
                let update_date = select_text_in(&elem, "dd span:nth-of-type(3), .s5, span.date");

                if title.is_empty() || href.is_empty() {
                    continue;
                }

                let book_id = href
                    .trim_end_matches('/')
                    .trim_end_matches(".html")
                    .rsplit('/')
                    .next()
                    .unwrap_or(&href)
                    .to_string();

                results.push(SearchResult {
                    site: site_name.to_string(),
                    book_id,
                    title,
                    author,
                    latest_chapter: latest,
                    update_date,
                    word_count: String::new(),
                });
            }

            if !results.is_empty() {
                break;
            }
        }
    }

    Ok(results)
}
