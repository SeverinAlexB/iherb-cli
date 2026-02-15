use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "iherb-cli",
    version,
    about = "Query iHerb product data from the command line"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Country code for localized pricing/availability (e.g., us, ch, de)
    #[arg(long, global = true)]
    pub country: Option<String>,

    /// Currency code (e.g., USD, CHF, EUR)
    #[arg(long, global = true)]
    pub currency: Option<String>,

    /// Bypass the local cache and fetch fresh data
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Delay between requests in milliseconds (default: 2000)
    #[arg(long, global = true)]
    pub delay: Option<u64>,

    /// Run browser in headed mode for troubleshooting
    #[arg(long, global = true)]
    pub debug: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Search for products on iHerb
    Search {
        /// Search term (e.g., "vitamin c", "omega 3")
        query: String,

        /// Max number of results to return (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Sort order: relevance, price-asc, price-desc, rating, best-selling
        #[arg(long, default_value = "relevance")]
        sort: String,

        /// Filter by category (e.g., supplements, vitamins, protein)
        #[arg(long)]
        category: Option<String>,
    },

    /// Get detailed product information
    Product {
        /// Numeric product ID or full iHerb product URL
        id_or_url: String,

        /// Only show a specific section: overview, ingredients, nutrition, reviews
        #[arg(long)]
        section: Option<String>,
    },
}
