use anyhow::Result;
use dialoguer::{Input, Select};
use futures::StreamExt;
use std::path::PathBuf;

use novel_downloader::{
    download_novel, list_providers, search_stream, download_stream,
    client::HttpClient,
    providers::get_all_providers,
    types::SearchResult,
    DownloadEvent,
};

fn main() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async_main())
}

async fn async_main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("search-first") => {
            let keyword = args.get(2).expect("Usage: novel-downloader search-first <keyword>");
            search_first(keyword).await?;
        }
        Some("search") => {
            let keyword = args
                .get(2)
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    Input::<String>::new()
                        .with_prompt("Enter novel name")
                        .interact_text()
                        .unwrap()
                });
            search_and_download(&keyword).await?;
        }
        Some("download") => {
            let provider_name = args.get(2).expect("Usage: novel-downloader download <provider> <book_id>");
            let book_id = args.get(3).expect("Usage: novel-downloader download <provider> <book_id>");
            let output = args.get(4).map(PathBuf::from);
            let client = HttpClient::new()?;
            let all_providers = get_all_providers();
            let provider = all_providers
                .iter()
                .find(|p| p.name() == provider_name)
                .unwrap_or_else(|| panic!("Provider '{}' not found", provider_name));
            download_novel(provider.as_ref(), &client, book_id, output.as_deref()).await?;
        }
        Some("list") => {
            let providers = list_providers();
            println!("Available providers ({} total):\n", providers.len());
            let with_search: Vec<_> = providers.iter().filter(|p| p.supports_search).collect();
            let without: Vec<_> = providers.iter().filter(|p| !p.supports_search).collect();
            println!("=== With Search ({}) ===", with_search.len());
            for p in &with_search { println!("  {} ({})", p.name, p.url); }
            println!("\n=== Without Search ({}) ===", without.len());
            for p in &without { println!("  {} ({})", p.name, p.url); }
        }
        _ => {
            println!("=== Novel Downloader ===");
            println!("Supported sites: {} providers\n", list_providers().len());
            let keyword: String = Input::new()
                .with_prompt("Enter novel name to search")
                .interact_text()?;
            search_and_download(&keyword).await?;
        }
    }

    Ok(())
}

/// Interactive: stream search results, prompt user, download selected novel.
async fn search_and_download(keyword: &str) -> Result<()> {
    let all_providers = get_all_providers();
    let search_count = all_providers.iter().filter(|p| p.supports_search()).count();
    println!("Searching '{}' across {} providers (streaming results)...\n", keyword, search_count);

    let mut stream = search_stream(keyword);
    let mut all_results: Vec<SearchResult> = Vec::new();

    while let Some(r) = stream.next().await {
        println!("[{:3}] {} - {} [{}]", all_results.len() + 1, r.title, r.author, r.site);
        all_results.push(r);
    }

    if all_results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    println!("\n{} results total.", all_results.len());

    let items: Vec<String> = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| format!("[{:3}] {} - {} [{}]", i + 1, r.title, r.author, r.site))
        .collect();

    let selection = Select::new()
        .with_prompt("Select a novel to download")
        .items(&items)
        .interact()?;

    let selected = &all_results[selection];
    println!("\nSelected: {} by {} from {}\n", selected.title, selected.author, selected.site);

    // Use download_stream → write TXT file
    let safe_name = selected.title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let output_path = PathBuf::from(format!("{}.txt", safe_name));
    let mut file = std::fs::File::create(&output_path)?;

    use std::io::Write;
    let mut stream = download_stream(&selected.site, &selected.book_id);
    let mut pb: Option<indicatif::ProgressBar> = None;

    while let Some(event) = stream.next().await {
        match event? {
            DownloadEvent::BookInfo { title, author, summary, total } => {
                write!(file, "《{}》\n作者：{}\n\n{}\n\n", title, author, summary)?;
                let bar = indicatif::ProgressBar::new(total as u64);
                bar.set_style(
                    indicatif::ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})"
                    ).unwrap().progress_chars("#>-")
                );
                pb = Some(bar);
            }
            DownloadEvent::Chapter { volume, title, content, .. } => {
                if let Some(vol) = volume {
                    write!(file, "\n\n{}\n\n", vol)?;
                }
                write!(file, "{}\n\n{}\n\n", title, content)?;
                if let Some(ref bar) = pb { bar.inc(1); }
            }
            DownloadEvent::ChapterError { title, .. } => {
                write!(file, "{}\n\n[下载失败]\n\n", title)?;
                if let Some(ref bar) = pb { bar.inc(1); }
            }
            DownloadEvent::Done { downloaded, failed } => {
                if let Some(ref bar) = pb { bar.finish_and_clear(); }
                println!("✓ {} → {} ({} 章, {} 失败)", selected.title, output_path.display(), downloaded, failed);
            }
        }
    }

    Ok(())
}

/// Non-interactive: print "provider\tbook_id\ttitle\tauthor" for best match.
async fn search_first(keyword: &str) -> Result<()> {
    let mut stream = search_stream(keyword);
    let mut all_results: Vec<SearchResult> = Vec::new();
    while let Some(r) = stream.next().await {
        all_results.push(r);
    }

    if all_results.is_empty() {
        eprintln!("NOT_FOUND:{}", keyword);
        return Ok(());
    }

    let kw_lower = keyword.to_lowercase();
    let best = all_results
        .iter()
        .find(|r| r.title.to_lowercase() == kw_lower)
        .or_else(|| all_results.iter().find(|r| r.title.to_lowercase().contains(&kw_lower)))
        .unwrap_or(&all_results[0]);

    println!("{}\t{}\t{}\t{}", best.site, best.book_id, best.title, best.author);
    Ok(())
}
