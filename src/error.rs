use thiserror::Error;

#[derive(Error, Debug)]
pub enum IherbError {
    #[error("Failed to launch browser: {0}")]
    BrowserLaunch(String),

    #[error("Browser navigation failed: {0}")]
    Navigation(String),

    #[error("Cloudflare challenge could not be solved after {0} attempts")]
    CloudflareBlocked(u32),

    #[error("Product not found: {0}")]
    ProductNotFound(String),

    #[error("Chrome download failed: {0}")]
    ChromeDownload(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
