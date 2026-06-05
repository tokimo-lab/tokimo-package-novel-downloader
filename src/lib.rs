#![allow(unused_imports, dead_code, clippy::field_reassign_with_default)]

pub mod client;
pub mod download;
pub mod provider;
pub mod providers;
pub mod types;
pub mod utils;

pub use download::stream_download;
pub use types::{BookInfo, Chapter, ChapterInfo, DownloadEvent, SearchResult, Volume};

use futures::Stream;
use serde::Serialize;
use tokio_stream::wrappers::ReceiverStream;

// ── ProviderMeta ─────────────────────────────────────────────────────────────

/// Lightweight metadata about one provider.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderMeta {
    pub name: String,
    pub url: String,
    pub supports_search: bool,
}

/// Returns metadata for all supported providers.
pub fn list_providers() -> Vec<ProviderMeta> {
    providers::get_all_providers()
        .into_iter()
        .map(|p| ProviderMeta {
            name: p.name().to_string(),
            url: p.base_url().to_string(),
            supports_search: p.supports_search(),
        })
        .collect()
}

// ── search_stream ─────────────────────────────────────────────────────────────

/// Search all providers in parallel.
///
/// Results stream in as each provider responds — faster providers appear first.
/// The stream closes automatically once all providers have replied.
///
/// # Example
/// ```no_run
/// use futures::StreamExt;
/// # tokio_test::block_on(async {
/// let mut stream = novel_downloader::search_stream("斗罗大陆");
/// while let Some(r) = stream.next().await {
///     println!("{} - {} [{}]", r.title, r.author, r.site);
/// }
/// # });
/// ```
pub fn search_stream(
    query: impl Into<String>,
) -> impl Stream<Item = SearchResult> + Send + 'static {
    let query = query.into();
    let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(256);

    // One task per provider: each owns its Box<dyn Provider> + cloned client.
    let all_providers = providers::get_all_providers();
    let client = match client::HttpClient::new() {
        Ok(c) => c,
        Err(_) => return ReceiverStream::new(rx),
    };

    for provider in all_providers {
        if !provider.supports_search() {
            continue;
        }
        let tx = tx.clone();
        let q = query.clone();
        let c = client.clone();
        tokio::spawn(async move {
            let results = match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                provider.search(&c, &q, 10),
            )
            .await
            {
                Ok(Ok(r)) => r,
                _ => vec![],
            };
            for r in results {
                if tx.send(r).await.is_err() {
                    break;
                }
            }
        });
    }
    // Drop the original tx so the channel closes after all spawned tasks finish.
    drop(tx);

    ReceiverStream::new(rx)
}

// ── download_stream ───────────────────────────────────────────────────────────

/// Download a novel as a stream of structured events.
///
/// Events arrive in this order:
/// 1. [`DownloadEvent::BookInfo`] — title, author, summary, total chapter count
/// 2. [`DownloadEvent::Chapter`] / [`DownloadEvent::ChapterError`] — one per chapter, **in order**
/// 3. [`DownloadEvent::Done`] — downloaded + failed counts
///
/// Internally keeps `CONCURRENCY=12` requests in-flight; a BTreeMap buffer
/// guarantees ordered delivery even when chapters complete out of order.
///
/// # Example
/// ```no_run
/// use futures::StreamExt;
/// use novel_downloader::DownloadEvent;
/// # tokio_test::block_on(async {
/// let mut stream = novel_downloader::download_stream("dxmwx", "16296");
/// while let Some(Ok(event)) = stream.next().await {
///     match event {
///         DownloadEvent::BookInfo { title, total, .. } =>
///             println!("Downloading «{}» ({} chapters)", title, total),
///         DownloadEvent::Chapter { index, title, content, .. } =>
///             println!("[{}] {} ({} chars)", index, title, content.len()),
///         DownloadEvent::Done { downloaded, failed } =>
///             println!("Done: {} ok, {} failed", downloaded, failed),
///         _ => {}
///     }
/// }
/// # });
/// ```
pub fn download_stream(
    provider_id: impl Into<String>,
    book_id: impl Into<String>,
) -> impl Stream<Item = anyhow::Result<DownloadEvent>> + Send + 'static {
    let provider_id = provider_id.into();
    let book_id = book_id.into();
    let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<DownloadEvent>>(64);

    tokio::spawn(async move {
        let client = match client::HttpClient::new() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return;
            }
        };
        let all_providers = providers::get_all_providers();
        let provider = match all_providers.into_iter().find(|p| p.name() == provider_id) {
            Some(p) => p,
            None => {
                let _ = tx
                    .send(Err(anyhow::anyhow!("Provider '{}' not found", provider_id)))
                    .await;
                return;
            }
        };
        // provider is owned (Box<dyn Provider + Send + Sync>) — safe to move into task
        download::stream_download(provider.as_ref(), &client, &book_id, tx).await;
    });

    ReceiverStream::new(rx)
}

// ── get_book_info ─────────────────────────────────────────────────────────────

/// Fetch book metadata (title, author, chapter list) for one provider + book_id.
pub async fn get_book_info(
    provider_id: impl AsRef<str>,
    book_id: impl AsRef<str>,
) -> anyhow::Result<BookInfo> {
    let client = client::HttpClient::new()?;
    let all_providers = providers::get_all_providers();
    let provider = all_providers
        .iter()
        .find(|p| p.name() == provider_id.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", provider_id.as_ref()))?;
    provider.get_book_info(&client, book_id.as_ref()).await
}

// ── get_chapter ───────────────────────────────────────────────────────────────

/// Fetch a single chapter's content from a specific provider.
pub async fn get_chapter(
    provider_id: impl AsRef<str>,
    book_id: impl AsRef<str>,
    chapter_id: impl AsRef<str>,
) -> anyhow::Result<Chapter> {
    let client = client::HttpClient::new()?;
    let all_providers = providers::get_all_providers();
    let provider = all_providers
        .iter()
        .find(|p| p.name() == provider_id.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", provider_id.as_ref()))?;
    provider
        .get_chapter_content(&client, book_id.as_ref(), chapter_id.as_ref())
        .await
}
