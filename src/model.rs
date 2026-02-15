use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSummary {
    pub name: String,
    pub brand: String,
    pub price: f64,
    pub original_price: Option<f64>,
    pub currency: String,
    pub rating: Option<f64>,
    pub review_count: Option<u32>,
    pub product_url: String,
    pub product_id: String,
    pub in_stock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductDetail {
    pub name: String,
    pub brand: String,
    pub price: f64,
    pub original_price: Option<f64>,
    pub currency: String,
    pub rating: Option<f64>,
    pub review_count: Option<u32>,
    pub product_url: String,
    pub product_id: String,
    pub in_stock: bool,
    pub description: Option<String>,
    pub product_code: Option<String>,
    pub upc: Option<String>,
    pub ingredients: Option<String>,
    pub supplement_facts: Option<SupplementFacts>,
    pub suggested_use: Option<String>,
    pub warnings: Option<String>,
    pub shipping_weight: Option<String>,
    pub category_breadcrumb: Option<Vec<String>>,
    pub review_distribution: Option<ReviewDistribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplementFacts {
    pub serving_size: Option<String>,
    pub servings_per_container: Option<String>,
    pub nutrients: Vec<Nutrient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nutrient {
    pub name: String,
    pub amount: String,
    pub daily_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDistribution {
    pub five_star: Option<f64>,
    pub four_star: Option<f64>,
    pub three_star: Option<f64>,
    pub two_star: Option<f64>,
    pub one_star: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub total_results: Option<u32>,
    pub products: Vec<ProductSummary>,
}
