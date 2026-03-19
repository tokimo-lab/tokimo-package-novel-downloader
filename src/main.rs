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
    let search_providers: Vec<_> = providers.iter().filter(|p| p.supports_search()).collect();
    println!(
        "Searching '{}' across {} providers...\n",
        keyword,
        search_providers.len()
    );

    // Search all providers concurrently
    let futures: Vec<_> = search_providers
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
                    Ok(Ok(results)) => {
                        if !results.is_empty() {
                            eprint!("  ✓ {} ({} results)\n", name, results.len());
                        }
                        results
                    }
                    Ok(Err(_e)) => {
                        vec![]
                    }
                    Err(_) => {
                        vec![]
                    }
                }
            }
        })
        .collect();

    let results = join_all(futures).await;
    let all_results: Vec<_> = results.into_iter().flatten().collect();

    if all_results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    println!("\nFound {} results:\n", all_results.len());
    let items: Vec<String> = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "[{:2}] {} - {} [{}]",
                i + 1,
                r.title,
                r.author,
                r.site
            )
        })
        .collect();

    for item in &items {
        println!("{}", item);
    }
    println!();

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
