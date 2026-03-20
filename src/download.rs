use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::client::HttpClient;
use crate::provider::Provider;

/// Download a novel and save as TXT.
///
/// Uses a streaming pipeline:
/// - Always keep `CONCURRENCY` chapter requests in-flight simultaneously
/// - An ordered buffer ensures chapters are written in correct order
///   even if they arrive out-of-order (fast chapters don't wait for slow ones)
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

    // Flatten all chapters across volumes into an indexed list
    // Each entry: (global_index, volume_name_if_first_in_vol, chapter_info)
    #[derive(Clone)]
    struct FlatChapter {
        idx: usize,
        vol_header: Option<String>, // write volume header before this chapter
        chapter_id: String,
        title: String,
    }

    let mut flat: Vec<FlatChapter> = Vec::new();
    let mut idx = 0usize;
    for volume in &book_info.volumes {
        let mut first_in_vol = true;
        for ch in &volume.chapters {
            let vol_header = if first_in_vol && !volume.volume_name.is_empty() {
                first_in_vol = false;
                Some(volume.volume_name.clone())
            } else {
                None
            };
            flat.push(FlatChapter {
                idx,
                vol_header,
                chapter_id: ch.chapter_id.clone(),
                title: ch.title.clone(),
            });
            idx += 1;
        }
    }

    let total = flat.len();
    println!("Chapters: {}", total);

    if total == 0 {
        println!("No chapters found.");
        return Ok(());
    }

    let output_path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        let safe_name = book_info.book_name
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        format!("{}.txt", safe_name).into()
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
    write!(
        file,
        "《{}》\n作者：{}\n\n{}\n\n",
        book_info.book_name, book_info.author, book_info.summary
    )?;

    // Streaming pipeline with ordered write buffer
    //
    //  flat[] ──[enqueue up to CONCURRENCY]──► FuturesUnordered
    //                                              │
    //                                    (idx, result) as each completes
    //                                              │
    //                                         BTreeMap<idx, ...>  (buffer)
    //                                              │
    //                             write consecutive entries from next_write_idx up

    type BoxFut<'a> = std::pin::Pin<Box<dyn std::future::Future<Output=(usize,Option<String>,String,Result<crate::types::Chapter>)> + Send + 'a>>;
    let mut futs: FuturesUnordered<BoxFut<'_>> = FuturesUnordered::new();
    let mut enqueue_ptr = 0usize; // next chapter in flat[] to enqueue
    // ordered buffer: idx → (vol_header, title, content_or_error)
    let mut buffer: BTreeMap<usize, (Option<String>, String, String)> = BTreeMap::new();
    let mut next_write = 0usize;
    let mut downloaded = 0u64;
    let mut failed = 0u64;

    // Seed initial batch
    while enqueue_ptr < total && futs.len() < CONCURRENCY {
        let fc = flat[enqueue_ptr].clone();
        enqueue_ptr += 1;
        futs.push(Box::pin(async move {
            let result = provider.get_chapter_content(client, book_id, &fc.chapter_id).await;
            (fc.idx, fc.vol_header, fc.title, result)
        }));
    }

    // Drain the stream
    while let Some((ch_idx, vol_header, fallback_title, result)) = futs.next().await {
        let (title, content) = match result {
            Ok(ch) => {
                let t = if ch.title.is_empty() { fallback_title } else { ch.title };
                (t, ch.content)
            }
            Err(_) => (fallback_title, "[下载失败]".to_string()),
        };
        buffer.insert(ch_idx, (vol_header, title, content));

        // Write all consecutive chapters that are ready
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

        // Enqueue the next chapter to keep pipeline full
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

    println!(
        "✓ {} → {} ({} 章, {} 失败)",
        book_info.book_name,
        output_path.display(),
        downloaded,
        failed
    );

    Ok(())
}
