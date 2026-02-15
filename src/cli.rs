use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "iherb-cli",
    version,
    about = "Query iHerb product data from the command line"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Country subdomain to use (e.g., us, ch, de). Note: iHerb may override based on your IP
    #[arg(long, global = true)]
    pub country: Option<String>,

    /// Fallback currency label when auto-detection fails (e.g., USD, CHF, EUR)
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
        #[arg(long, value_enum, default_value_t = SortOrder::Relevance)]
        sort: SortOrder,

        /// Filter by category (e.g., supplements, vitamins, protein)
        #[arg(long)]
        category: Option<String>,
    },

    /// Get detailed product information
    Product {
        /// Numeric product ID or full iHerb product URL
        id_or_url: String,

        /// Only show a specific section: overview, description, ingredients, nutrition, suggested-use, warnings, reviews
        #[arg(long, value_enum)]
        section: Option<Section>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortOrder {
    Relevance,
    #[value(name = "price-asc")]
    PriceAsc,
    #[value(name = "price-desc")]
    PriceDesc,
    Rating,
    #[value(name = "best-selling")]
    BestSelling,
}

impl SortOrder {
    pub fn as_url_param(self) -> &'static str {
        match self {
            SortOrder::Relevance => "",
            SortOrder::PriceAsc => "&sr=4",
            SortOrder::PriceDesc => "&sr=3",
            SortOrder::Rating => "&sr=1",
            SortOrder::BestSelling => "&sr=2",
        }
    }

    pub fn as_cache_key(self) -> &'static str {
        match self {
            SortOrder::Relevance => "relevance",
            SortOrder::PriceAsc => "price-asc",
            SortOrder::PriceDesc => "price-desc",
            SortOrder::Rating => "rating",
            SortOrder::BestSelling => "best-selling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Section {
    Overview,
    Description,
    Nutrition,
    Ingredients,
    #[value(name = "suggested-use")]
    SuggestedUse,
    Warnings,
    Reviews,
}

impl Section {
    pub const ALL: &[Section] = &[
        Section::Overview,
        Section::Description,
        Section::Nutrition,
        Section::Ingredients,
        Section::SuggestedUse,
        Section::Warnings,
        Section::Reviews,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Section::Overview => "overview",
            Section::Description => "description",
            Section::Nutrition => "nutrition",
            Section::Ingredients => "ingredients",
            Section::SuggestedUse => "suggested use",
            Section::Warnings => "warnings",
            Section::Reviews => "review",
        }
    }
}
