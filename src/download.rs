use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;
use std::path::Path;

use crate::client::HttpClient;
use crate::provider::Provider;

/// Download a novel and save as TXT (writes incrementally)
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
        let filename = format!("{}.txt", book_info.book_name);
        Path::new(&filename).to_path_buf()
    };

    let pb = ProgressBar::new(total_chapters as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    // Write incrementally to file
    let mut file = std::fs::File::create(&output_path)?;
    write!(
        file,
        "《{}》\n作者：{}\n\n{}\n\n",
        book_info.book_name, book_info.author, book_info.summary
    )?;

    let mut downloaded = 0;
    let mut failed = 0;

    for volume in &book_info.volumes {
        if !volume.volume_name.is_empty() {
            write!(file, "\n\n{}\n\n", volume.volume_name)?;
        }

        for chapter_info in &volume.chapters {
            match provider
                .get_chapter_content(client, book_id, &chapter_info.chapter_id)
                .await
            {
                Ok(chapter) => {
                    write!(file, "{}\n\n{}\n\n", chapter.title, chapter.content)?;
                    downloaded += 1;
                }
                Err(e) => {
                    write!(
                        file,
                        "{}\n\n[下载失败: {}]\n\n",
                        chapter_info.title, e
                    )?;
                    failed += 1;
                }
            }
            pb.inc(1);

            // Small delay to avoid being rate limited
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    pb.finish_with_message("Download complete");

    println!(
        "\nSaved to: {} ({} chapters downloaded, {} failed)",
        output_path.display(),
        downloaded,
        failed
    );

    Ok(())
}
