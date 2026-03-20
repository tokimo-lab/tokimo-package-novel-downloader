#![allow(unused_imports, dead_code, unused_variables, unused_mut)]

mod client;
mod download;
mod provider;
mod providers;
mod types;
mod utils;

use anyhow::Result;
use dialoguer::{Input, Select};
use futures::future::join_all;
use std::path::PathBuf;

use crate::client::HttpClient;
use crate::provider::Provider;

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_main())
}

async fn async_main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let client = HttpClient::new()?;
    let all_providers = providers::get_all_providers();

    match args.get(1).map(|s| s.as_str()) {
        Some("search-first") => {
            // Non-interactive: print "provider book_id title" for best match
            let keyword = args.get(2).expect("Usage: novel-downloader search-first <keyword>");
            search_first(&client, &all_providers, keyword).await?;
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
            search_and_download(&client, &all_providers, &keyword).await?;
        }
        Some("download") => {
            let provider_name = args.get(2).expect("Usage: novel-downloader download <provider> <book_id>");
            let book_id = args.get(3).expect("Usage: novel-downloader download <provider> <book_id>");
            let output = args.get(4).map(PathBuf::from);
            let provider = all_providers
                .iter()
                .find(|p| p.name() == provider_name)
                .unwrap_or_else(|| panic!("Provider '{}' not found", provider_name));
            download::download_novel(provider.as_ref(), &client, book_id, output.as_deref()).await?;
        }
        Some("list") => {
            println!("Available providers ({} total):\n", all_providers.len());
            let with_search: Vec<_> = all_providers.iter().filter(|p| p.supports_search()).collect();
            let without_search: Vec<_> = all_providers.iter().filter(|p| !p.supports_search()).collect();

            println!("=== With Search ({}) ===", with_search.len());
            for p in &with_search {
                println!("  {} ({})", p.name(), p.base_url());
            }
            println!("\n=== Without Search ({}) ===", without_search.len());
            for p in &without_search {
                println!("  {} ({})", p.name(), p.base_url());
            }
        }
        _ => {
            // Interactive mode
            println!("=== Novel Downloader ===");
            println!("Supported sites: {} providers\n", all_providers.len());
            let keyword: String = Input::new()
                .with_prompt("Enter novel name to search")
                .interact_text()?;
            search_and_download(&client, &all_providers, &keyword).await?;
        }
    }

    Ok(())
}

async fn search_and_download(
    client: &HttpClient,
    providers: &[Box<dyn Provider>],
    keyword: &str,
) -> Result<()> {
    use futures::StreamExt;
    use futures::stream::FuturesUnordered;

    let search_providers: Vec<_> = providers.iter().filter(|p| p.supports_search()).collect();
    println!(
        "Searching '{}' across {} providers (streaming results)...\n",
        keyword,
        search_providers.len()
    );

    // Stream results: print each provider's results the moment they arrive
    let mut futs: FuturesUnordered<_> = search_providers
        .iter()
        .map(|p| {
            let name = p.name().to_string();
            async move {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    p.search(client, keyword, 5),
                )
                .await
                {
                    Ok(Ok(results)) => (name, results),
                    Ok(Err(_)) => (name, vec![]),
                    Err(_) => (name, vec![]),
                }
            }
        })
        .collect();

    let mut all_results: Vec<crate::types::SearchResult> = Vec::new();

    // Results stream in as each provider finishes
    while let Some((site_name, results)) = futs.next().await {
        if !results.is_empty() {
            for r in &results {
                println!(
                    "[{:3}] {} - {} [{}]",
                    all_results.len() + 1,
                    r.title,
                    r.author,
                    site_name
                );
            }
            all_results.extend(results);
        }
    }

    if all_results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    println!("\n{} results total. Enter number to download: ", all_results.len());

    // Rebuild display list for dialoguer
    let items: Vec<String> = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "[{:3}] {} - {} [{}]",
                i + 1,
                r.title,
                r.author,
                r.site
            )
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select a novel to download")
        .items(&items)
        .interact()?;

    let selected = &all_results[selection];
    println!(
        "\nSelected: {} by {} from {}\n",
        selected.title, selected.author, selected.site
    );

    let provider = providers
        .iter()
        .find(|p| p.name() == selected.site)
        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", selected.site))?;

    download::download_novel(provider.as_ref(), client, &selected.book_id, None).await?;

    Ok(())
}

/// Non-interactive search: prints "provider\tbook_id\ttitle\tauthor" for best result
async fn search_first(
    client: &HttpClient,
    providers: &[Box<dyn Provider>],
    keyword: &str,
) -> Result<()> {
    let search_providers: Vec<_> = providers.iter().filter(|p| p.supports_search()).collect();

    let futures: Vec<_> = search_providers
        .iter()
        .map(|p| async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(12),
                p.search(client, keyword, 5),
            )
            .await
            {
                Ok(Ok(results)) => results,
                _ => vec![],
            }
        })
        .collect();

    let results = join_all(futures).await;
    let all_results: Vec<_> = results.into_iter().flatten().collect();

    if all_results.is_empty() {
        eprintln!("NOT_FOUND:{}", keyword);
        return Ok(());
    }

    // Prefer exact title match, then partial match
    let kw_lower = keyword.to_lowercase();
    let best = all_results
        .iter()
        .find(|r| r.title.to_lowercase() == kw_lower)
        .or_else(|| {
            all_results
                .iter()
                .find(|r| r.title.to_lowercase().contains(&kw_lower))
        })
        .unwrap_or(&all_results[0]);

    // Output tab-separated: provider  book_id  title  author
    println!("{}\t{}\t{}\t{}", best.site, best.book_id, best.title, best.author);
    Ok(())
}
