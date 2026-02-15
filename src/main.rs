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
use cli::{Cli, Commands, Section, SortOrder};
use config::AppConfig;

use crate::browser::session::BrowserSession;
use crate::cache::Cache;
use crate::scraper::navigation::Navigator;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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

    let config = AppConfig::load(
        cli.country,
        cli.currency,
        cli.no_cache,
        cli.delay,
        cli.debug,
    )?;

    ctrlc::set_handler(|| {
        eprintln!("\nInterrupted. Cleaning up...");
        std::process::exit(130);
    })
    .context("Failed to set Ctrl+C handler")?;

    let mut browser_session: Option<BrowserSession> = None;

    match cli.command {
        Commands::Search {
            query,
            limit,
            sort,
            category,
        } => {
            cmd_search(
                &config,
                &mut browser_session,
                &query,
                limit,
                sort,
                category.as_deref(),
            )
            .await?;
        }
        Commands::Product { id_or_url, section } => {
            cmd_product(&config, &mut browser_session, &id_or_url, section).await?;
        }
    }

    if let Some(session) = browser_session.take() {
        if let Err(e) = session.close().await {
            tracing::warn!("Failed to close browser: {}", e);
        }
    }

    Ok(())
}

async fn cmd_search(
    config: &AppConfig,
    browser_session: &mut Option<BrowserSession>,
    query: &str,
    limit: usize,
    sort: SortOrder,
    category: Option<&str>,
) -> Result<()> {
    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    if let Some(cached) = cache.get_search::<model::SearchResult>(query, sort, category) {
        let mut result = cached;
        result.products.truncate(limit);
        print!("{}", output::format_search_results(&result));
        return Ok(());
    }

    let session = get_or_launch_browser(config, browser_session).await?;
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

    if let Err(e) = cache.set_search(query, sort, category, &result) {
        tracing::debug!("Failed to cache search results: {}", e);
    }

    print!("{}", output::format_search_results(&result));
    Ok(())
}

async fn cmd_product(
    config: &AppConfig,
    browser_session: &mut Option<BrowserSession>,
    id_or_url: &str,
    section: Option<Section>,
) -> Result<()> {
    let product_id = parse_product_identifier(id_or_url)?;
    let cache = Cache::new(config.cache_dir.clone(), config.no_cache);

    if let Some(cached) = cache.get_product::<model::ProductDetail>(&product_id) {
        print!("{}", output::format_product_detail(&cached, section));
        return Ok(());
    }

    let session = get_or_launch_browser(config, browser_session).await?;
    let page = session.new_page().await?;
    let navigator = Navigator::new(config.delay_ms);

    let base_url = config.base_url();
    let url = format!("{}/pr/item/{}", base_url, product_id);

    let html = navigator
        .navigate_with_retry(&page, &url, 2)
        .await
        .context("Failed to navigate to product page")?;

    if scraper::helpers::is_not_found_page(&html) {
        anyhow::bail!("Product not found: {}", product_id);
    }

    let product =
        scraper::product::extract_product(&page, &html, &product_id, &base_url, &config.currency)
            .await
            .context("Failed to extract product data")?;

    // Validate the extracted product to catch nonexistent product pages that slip
    // through extraction (e.g., iHerb returns a page that doesn't trigger 404 detection
    // but has no real product data).
    if product.name.is_empty()
        || product.name == "Unknown Product"
        || (product.price == 0.0 && product.rating.is_none() && product.review_count.is_none())
    {
        anyhow::bail!("Product not found: {}", product_id);
    }

    if let Err(e) = cache.set_product(&product_id, &product) {
        tracing::debug!("Failed to cache product data: {}", e);
    }

    print!("{}", output::format_product_detail(&product, section));
    Ok(())
}

async fn get_or_launch_browser<'a>(
    config: &AppConfig,
    session: &'a mut Option<BrowserSession>,
) -> Result<&'a BrowserSession> {
    if session.is_none() {
        let chrome_path =
            browser::resolve::resolve_chrome(config.browser_path.as_ref(), &config.data_dir)
                .await
                .context("Failed to resolve Chrome browser")?;

        let launched = BrowserSession::launch(chrome_path, config)
            .await
            .context("Failed to launch browser")?;

        *session = Some(launched);
    }
    Ok(session.as_ref().unwrap())
}

fn parse_product_identifier(input: &str) -> Result<String> {
    if input.chars().all(|c| c.is_ascii_digit()) && !input.is_empty() {
        return Ok(input.to_string());
    }

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
