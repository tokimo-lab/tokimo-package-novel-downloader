use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::future::Future;

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::{BookInfo, Chapter, DownloadEvent};

// ── internal flat chapter list ──────────────────────────────────────────────

#[derive(Clone)]
struct FlatChapter {
    idx: usize,
    vol_header: Option<String>,
    chapter_id: String,
    title: String,
}

fn build_flat(book_info: &BookInfo) -> Vec<FlatChapter> {
    let mut flat = Vec::new();
    let mut idx = 0usize;
    for volume in &book_info.volumes {
        let mut first = true;
        for ch in &volume.chapters {
            flat.push(FlatChapter {
                idx,
                vol_header: if first && !volume.volume_name.is_empty() {
                    first = false;
                    Some(volume.volume_name.clone())
                } else {
                    None
                },
                chapter_id: ch.chapter_id.clone(),
                title: ch.title.clone(),
            });
            idx += 1;
        }
    }
    flat
}

type ChapterFut<'a> = Pin<Box<
    dyn Future<Output = (usize, Option<String>, String, Result<Chapter>)> + Send + 'a,
>>;

// ── public: streaming download ───────────────────────────────────────────────

/// Download a novel and stream `DownloadEvent` values via an mpsc channel.
///
/// Event order:
/// 1. `DownloadEvent::BookInfo` — book metadata + total chapter count
/// 2. `DownloadEvent::Chapter` / `DownloadEvent::ChapterError` — one per chapter, in-order
/// 3. `DownloadEvent::Done` — final summary
pub async fn stream_download(
    provider: &dyn Provider,
    client: &HttpClient,
    book_id: &str,
    tx: tokio::sync::mpsc::Sender<Result<DownloadEvent>>,
) {
    const CONCURRENCY: usize = 12;

    macro_rules! send {
        ($val:expr) => {
            if tx.send($val).await.is_err() {
                return;
            }
        };
    }

    let book_info = match provider.get_book_info(client, book_id).await {
        Ok(info) => info,
        Err(e) => { send!(Err(e)); return; }
    };

    let flat = build_flat(&book_info);
    let total = flat.len();

    send!(Ok(DownloadEvent::BookInfo {
        title: book_info.book_name.clone(),
        author: book_info.author.clone(),
        summary: book_info.summary.clone(),
        total,
    }));

    if total == 0 {
        send!(Ok(DownloadEvent::Done { downloaded: 0, failed: 0 }));
        return;
    }

    let mut buffer: BTreeMap<usize, (Option<String>, String, String, bool)> = BTreeMap::new();
    let mut futs: FuturesUnordered<ChapterFut<'_>> = FuturesUnordered::new();
    let mut enqueue_ptr = 0usize;
    let mut next_write = 0usize;
    let mut downloaded = 0usize;
    let mut failed = 0usize;

    while enqueue_ptr < total && futs.len() < CONCURRENCY {
        let fc = flat[enqueue_ptr].clone();
        enqueue_ptr += 1;
        futs.push(Box::pin(async move {
            let result = provider.get_chapter_content(client, book_id, &fc.chapter_id).await;
            (fc.idx, fc.vol_header, fc.title, result)
        }));
    }

    while let Some((ch_idx, vol_header, fallback_title, result)) = futs.next().await {
        let (title, content, ok) = match result {
            Ok(ch) => {
                let t = if ch.title.is_empty() { fallback_title } else { ch.title };
                (t, ch.content, true)
            }
            Err(e) => (fallback_title, e.to_string(), false),
        };
        buffer.insert(ch_idx, (vol_header, title, content, ok));

        while let Some((vh, t, c, ok)) = buffer.remove(&next_write) {
            if ok {
                send!(Ok(DownloadEvent::Chapter {
                    index: next_write,
                    volume: vh,
                    title: t,
                    content: c,
                }));
                downloaded += 1;
            } else {
                send!(Ok(DownloadEvent::ChapterError {
                    index: next_write,
                    title: t,
                    error: c,
                }));
                failed += 1;
            }
            next_write += 1;
        }

        if enqueue_ptr < total {
            let fc = flat[enqueue_ptr].clone();
            enqueue_ptr += 1;
            futs.push(Box::pin(async move {
                let result = provider.get_chapter_content(client, book_id, &fc.chapter_id).await;
                (fc.idx, fc.vol_header, fc.title, result)
            }));
        }
    }

    send!(Ok(DownloadEvent::Done { downloaded, failed }));
}
