use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;

use crate::client::HttpClient;
use crate::provider::Provider;
use crate::types::{BookInfo, Chapter, DownloadEvent};

// ── internal flat chapter list ──────────────────────────────────────────────

#[derive(Clone)]
struct FlatChapter {
    idx: usize,
    vol_header: Option<String>, // volume header to print before this chapter
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

// ── type alias for the boxed chapter future ──────────────────────────────────

type ChapterFut<'a> = Pin<Box<
    dyn Future<Output = (usize, Option<String>, String, Result<Chapter>)> + Send + 'a,
>>;

// ── public: download to TXT file (used by the CLI binary) ────────────────────

/// Download a novel and save as a TXT file.
///
/// Uses a streaming pipeline:
/// - Always keep `CONCURRENCY` chapter requests in-flight simultaneously.
/// - A `BTreeMap` buffer ensures chapters are written in correct order even if
///   they arrive out-of-order (fast chapters don't block slow ones).
pub async fn download_novel(
    provider: &dyn Provider,
    client: &HttpClient,
    book_id: &str,
    output: Option<&Path>,
) -> Result<()> {
    const CONCURRENCY: usize = 12;

    println!("Fetching book info from {}...", provider.name());
    let book_info = provider.get_book_info(client, book_id).await?;

    println!("Title: {}", book_info.book_name);
    println!("Author: {}", book_info.author);

    let flat = build_flat(&book_info);
    let total = flat.len();
    println!("Chapters: {}", total);

    if total == 0 {
        println!("No chapters found.");
        return Ok(());
    }

    let output_path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        let safe = book_info.book_name
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        format!("{}.txt", safe).into()
    };

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message(book_info.book_name.clone());

    let mut file = std::fs::File::create(&output_path)?;
    write!(file, "《{}》\n作者：{}\n\n{}\n\n", book_info.book_name, book_info.author, book_info.summary)?;

    let mut futs: FuturesUnordered<ChapterFut<'_>> = FuturesUnordered::new();
    let mut enqueue_ptr = 0usize;
    let mut buffer: BTreeMap<usize, (Option<String>, String, String)> = BTreeMap::new();
    let mut next_write = 0usize;
    let mut downloaded = 0u64;
    let mut failed = 0u64;

    while enqueue_ptr < total && futs.len() < CONCURRENCY {
        let fc = flat[enqueue_ptr].clone();
        enqueue_ptr += 1;
        futs.push(Box::pin(async move {
            let result = provider.get_chapter_content(client, book_id, &fc.chapter_id).await;
            (fc.idx, fc.vol_header, fc.title, result)
        }));
    }

    while let Some((ch_idx, vol_header, fallback_title, result)) = futs.next().await {
        let (title, content) = match result {
            Ok(ch) => {
                let t = if ch.title.is_empty() { fallback_title } else { ch.title };
                (t, ch.content)
            }
            Err(_) => (fallback_title, "[下载失败]".to_string()),
        };
        buffer.insert(ch_idx, (vol_header, title, content));

        while let Some((vh, t, c)) = buffer.remove(&next_write) {
            if let Some(header) = vh {
                write!(file, "\n\n{}\n\n", header)?;
            }
            if c == "[下载失败]" {
                write!(file, "{}\n\n[下载失败]\n\n", t)?;
                failed += 1;
            } else {
                write!(file, "{}\n\n{}\n\n", t, c)?;
                downloaded += 1;
            }
            next_write += 1;
            pb.inc(1);
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

    pb.finish_and_clear();
    println!("✓ {} → {} ({} 章, {} 失败)", book_info.book_name, output_path.display(), downloaded, failed);
    Ok(())
}

// ── public: streaming download (used by the library API) ─────────────────────

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

    // Buffer: idx → (vol_header, title, content, ok)
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
