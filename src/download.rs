use anyhow::Result;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::client::HttpClient;
use crate::provider::Provider;

/// Download a novel and save as TXT (concurrent chapters, writes in order)
pub async fn download_novel(
    provider: &dyn Provider,
    client: &HttpClient,
    book_id: &str,
    output: Option<&Path>,
) -> Result<()> {
    println!("Fetching book info from {}...", provider.name());
    let book_info = provider.get_book_info(client, book_id).await?;

    println!("Title: {}", book_info.book_name);
    println!("Author: {}", book_info.author);

    let total_chapters: usize = book_info.volumes.iter().map(|v| v.chapters.len()).sum();
    println!("Chapters: {}", total_chapters);

    if total_chapters == 0 {
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

    let pb = ProgressBar::new(total_chapters as u64);
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

    // 10 concurrent chapter downloads
    let sem = Arc::new(Semaphore::new(10));
    let mut downloaded = 0u64;
    let mut failed = 0u64;

    for volume in &book_info.volumes {
        if !volume.volume_name.is_empty() {
            write!(file, "\n\n{}\n\n", volume.volume_name)?;
        }

        // Process chapters in batches of 20
        for chunk in volume.chapters.chunks(20) {
            let futs: Vec<_> = chunk.iter().map(|ch_info| {
                let sem2 = sem.clone();
                let ch_id = ch_info.chapter_id.clone();
                let ch_title = ch_info.title.clone();
                async move {
                    let _permit = sem2.acquire().await.unwrap();
                    let result = provider.get_chapter_content(client, book_id, &ch_id).await;
                    (ch_id, ch_title, result)
                }
            }).collect();

            let results = join_all(futs).await;

            for (_ch_id, ch_title, result) in results {
                match result {
                    Ok(chapter) => {
                        let title = if chapter.title.is_empty() { ch_title } else { chapter.title };
                        write!(file, "{}\n\n{}\n\n", title, chapter.content)?;
                        downloaded += 1;
                    }
                    Err(_) => {
                        write!(file, "{}\n\n[下载失败]\n\n", ch_title)?;
                        failed += 1;
                    }
                }
                pb.inc(1);
            }
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
