mod browser;
mod cache;
mod cli;
mod config;
mod error;
mod model;
mod output;
mod scraper;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands};
use config::AppConfig;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::browser::session::BrowserSession;
use crate::cache::Cache;
use crate::scraper::navigation::Navigator;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up tracing
    let filter = if cli.debug {
        "iherb_cli=debug"
    } else {
        "iherb_cli=warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    // Load config
    let config = AppConfig::load(
        cli.country,
        cli.currency,
        cli.no_cache,
        cli.delay,
        cli.debug,
    )?;

    // Set up Ctrl+C handler
    let browser_ref: Arc<Mutex<Option<BrowserSession>>> = Arc::new(Mutex::new(None));
    let browser_cleanup = browser_ref.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nInterrupted. Cleaning up...");
        // We can't easily await here, so just exit.
        // The browser process will be killed when our process exits.
        std::process::exit(130);
    })
    .context("Failed to set Ctrl+C handler")?;

    match cli.command {
        Commands::Search {
            query,
            limit,
            sort,
            category,
        } => {
            cmd_search(
                &config,
                &browser_ref,
                &query,
                limit,
                &sort,
                category.as_deref(),
            )
            .await?;
        }
        Commands::Product { id_or_url, section } => {
            if let Some(ref sec) = section {
                validate_section(sec)?;
            }
            cmd_product(&config, &browser_ref, &id_or_url, section.as_deref()).await?;
        }
    }

    // Clean up browser
    if let Some(session) = browser_cleanup.lock().await.take() {
        let _ = session.close().await;
    }

    Ok(())
}

async fn cmd_search(
    config: &AppConfig,
    browser_ref: &Arc<Mutex<Option<BrowserSession>>>,
    query: &str,
    limit: usize,
    sort: &str,
    category: Option<&str>,
) -> Result<()> {
    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    // Check cache
    if let Some(cached) = cache.get_search::<model::SearchResult>(query, sort, category) {
        let mut result = cached;
        result.products.truncate(limit);
        print!("{}", output::format_search_results(&result));
        return Ok(());
    }

    // Launch browser
    let session = get_or_launch_browser(config, browser_ref).await?;
    let page = session.new_page().await?;
    let navigator = Navigator::new(config.delay_ms);

    let base_url = config.base_url();
    let total_pages = scraper::search::pages_needed(limit);
    let mut all_products = Vec::new();
    let mut total_results = None;

    for page_num in 1..=total_pages {
        if all_products.len() >= limit {
            break;
        }

        let url = scraper::search::build_search_url(&base_url, query, sort, category, page_num);
        let html = navigator
            .navigate_with_retry(&page, &url, 2)
            .await
            .context("Failed to navigate to search page")?;

        let page_result =
            scraper::search::extract_search(&page, &html, query, &base_url, &config.currency)
                .await
                .context("Failed to extract search results")?;

        if page_result.products.is_empty() {
            break;
        }

        if total_results.is_none() {
            total_results = page_result.total_results;
        }

        all_products.extend(page_result.products);

        if page_num < total_pages {
            navigator.rate_limit_delay().await;
        }
    }

    if all_products.is_empty() {
        anyhow::bail!("No search results found for: {}", query);
    }

    all_products.truncate(limit);

    let result = model::SearchResult {
        query: query.to_string(),
        total_results,
        products: all_products,
    };

    // Cache result
    let _ = cache.set_search(query, sort, category, &result);

    print!("{}", output::format_search_results(&result));
    Ok(())
}

async fn cmd_product(
    config: &AppConfig,
    browser_ref: &Arc<Mutex<Option<BrowserSession>>>,
    id_or_url: &str,
    section: Option<&str>,
) -> Result<()> {
    let product_id = parse_product_identifier(id_or_url)?;
    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    // Check cache
    if let Some(cached) = cache.get_product::<model::ProductDetail>(&product_id) {
        print!("{}", output::format_product_detail(&cached, section));
        return Ok(());
    }

    // Launch browser
    let session = get_or_launch_browser(config, browser_ref).await?;
    let page = session.new_page().await?;
    let navigator = Navigator::new(config.delay_ms);

    let base_url = config.base_url();
    let url = format!("{}/pr/item/{}", base_url, product_id);

    let html = navigator
        .navigate_with_retry(&page, &url, 2)
        .await
        .context("Failed to navigate to product page")?;

    // Check for 404
    if html.contains("Page Not Found") || html.contains("404 Not Found") {
        anyhow::bail!("Product not found: {}", product_id);
    }

    let product =
        scraper::product::extract_product(&page, &html, &product_id, &base_url, &config.currency)
            .await
            .context("Failed to extract product data")?;

    // Cache result
    let _ = cache.set_product(&product_id, &product);

    print!("{}", output::format_product_detail(&product, section));
    Ok(())
}

async fn get_or_launch_browser(
    config: &AppConfig,
    browser_ref: &Arc<Mutex<Option<BrowserSession>>>,
) -> Result<&'static BrowserSession> {
    // We need the browser session to live for the duration of the command.
    // Use a leaked reference approach for simplicity within a single CLI invocation.
    let mut guard = browser_ref.lock().await;
    if guard.is_none() {
        let chrome_path =
            browser::resolve::resolve_chrome(config.browser_path.as_ref(), &config.data_dir)
                .await
                .context("Failed to resolve Chrome browser")?;

        let session = BrowserSession::launch(chrome_path, config)
            .await
            .context("Failed to launch browser")?;

        *guard = Some(session);
    }
    // Safety: session lives until main() cleanup. We leak a reference that's valid
    // for the duration of this CLI invocation.
    let session_ref = guard.as_ref().unwrap() as *const BrowserSession;
    Ok(unsafe { &*session_ref })
}

fn parse_product_identifier(input: &str) -> Result<String> {
    // If it's a pure number, use it directly
    if input.chars().all(|c| c.is_ascii_digit()) && !input.is_empty() {
        return Ok(input.to_string());
    }

    // Try to extract ID from URL (last numeric path segment)
    if input.contains("iherb.com") {
        if let Some(id) = input
            .split('/')
            .rev()
            .find(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
        {
            return Ok(id.to_string());
        }
    }

    anyhow::bail!(
        "Invalid product identifier: {}. Use a numeric ID or full iHerb URL",
        input
    );
}

fn validate_section(section: &str) -> Result<()> {
    let valid = ["overview", "ingredients", "nutrition", "reviews"];
    if valid.contains(&section) {
        Ok(())
    } else {
        anyhow::bail!(
            "Invalid section: {}. Valid sections: overview, ingredients, nutrition, reviews",
            section
        );
    }
}
