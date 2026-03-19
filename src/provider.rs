use anyhow::Result;
use async_trait::async_trait;

use crate::client::HttpClient;
use crate::types::{BookInfo, Chapter, SearchResult};

#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider identifier (e.g., "biquge5")
    fn name(&self) -> &str;

    /// Display name for the provider
    fn display_name(&self) -> &str {
        self.name()
    }

    /// Base URL of the site
    fn base_url(&self) -> &str;

    /// Whether this provider supports search
    fn supports_search(&self) -> bool {
        false
    }

    /// Search for novels by keyword
    async fn search(
        &self,
        _client: &HttpClient,
        _keyword: &str,
        _limit: usize,
    ) -> Result<Vec<SearchResult>> {
        Ok(vec![])
    }

    /// Get book information including chapter list
    async fn get_book_info(&self, client: &HttpClient, book_id: &str) -> Result<BookInfo>;

    /// Get chapter content
    async fn get_chapter_content(
        &self,
        client: &HttpClient,
        book_id: &str,
        chapter_id: &str,
    ) -> Result<Chapter>;
}
