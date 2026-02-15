use crate::error::IherbError;
use chromiumoxide::Page;
use std::time::Duration;

const MAX_CLOUDFLARE_RETRIES: u32 = 3;
const CLOUDFLARE_WAIT_SECS: u64 = 12;

pub struct Navigator {
    delay_ms: u64,
}

impl Navigator {
    pub fn new(delay_ms: u64) -> Self {
        Self { delay_ms }
    }

    pub async fn navigate(&self, page: &Page, url: &str) -> Result<String, IherbError> {
        tracing::info!("Navigating to: {}", url);

        page.goto(url)
            .await
            .map_err(|e| IherbError::Navigation(format!("Failed to navigate to {}: {}", url, e)))?;

        // Wait for initial page load
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;

        // Wait for document.readyState === 'complete' (up to 10s)
        for _ in 0..20 {
            let ready = page
                .evaluate("document.readyState")
                .await
                .ok()
                .and_then(|v| v.into_value::<String>().ok())
                .unwrap_or_default();
            if ready == "complete" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Check for and handle Cloudflare challenge
        for attempt in 1..=MAX_CLOUDFLARE_RETRIES {
            if !self.is_cloudflare_challenge(page).await {
                break;
            }

            if attempt == MAX_CLOUDFLARE_RETRIES {
                return Err(IherbError::CloudflareBlocked(MAX_CLOUDFLARE_RETRIES));
            }

            tracing::info!(
                "Cloudflare challenge detected (attempt {}/{}), waiting up to {}s...",
                attempt,
                MAX_CLOUDFLARE_RETRIES,
                CLOUDFLARE_WAIT_SECS
            );

            // Try clicking the Cloudflare Turnstile checkbox (may fail due to cross-origin, but worth trying)
            let _ = page
                .evaluate(
                    r#"
                    try {
                        const iframe = document.querySelector('iframe[src*="challenges"]');
                        if (iframe && iframe.contentDocument) {
                            const checkbox = iframe.contentDocument.querySelector('input[type="checkbox"]');
                            if (checkbox) checkbox.click();
                        }
                    } catch(e) {}
                    "#,
                )
                .await;

            // Wait for Cloudflare to resolve, but check periodically for early exit
            let check_interval_ms = 1000;
            let total_checks = (CLOUDFLARE_WAIT_SECS * 1000) / check_interval_ms;
            for _ in 0..total_checks {
                tokio::time::sleep(Duration::from_millis(check_interval_ms)).await;
                if !self.is_cloudflare_challenge(page).await {
                    tracing::info!("Cloudflare challenge resolved early");
                    break;
                }
            }
        }

        let html = page
            .content()
            .await
            .map_err(|e| IherbError::Navigation(format!("Failed to get page content: {}", e)))?;

        Ok(html)
    }

    pub async fn navigate_with_retry(
        &self,
        page: &Page,
        url: &str,
        max_retries: u32,
    ) -> Result<String, IherbError> {
        let mut last_err = None;

        for attempt in 1..=max_retries + 1 {
            match self.navigate(page, url).await {
                Ok(html) => return Ok(html),
                Err(e) => {
                    tracing::warn!(
                        "Navigation attempt {}/{} failed: {}",
                        attempt,
                        max_retries + 1,
                        e
                    );
                    last_err = Some(e);
                    if attempt <= max_retries {
                        let backoff = Duration::from_secs(2u64.pow(attempt - 1));
                        tracing::info!("Retrying in {:?}...", backoff);
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(last_err.unwrap())
    }

    async fn is_cloudflare_challenge(&self, page: &Page) -> bool {
        match page.evaluate("document.title").await {
            Ok(val) => {
                let title = val.into_value::<String>().unwrap_or_default();
                title.contains("Just a moment") || title.contains("Attention Required")
            }
            Err(_) => false,
        }
    }

    pub async fn rate_limit_delay(&self) {
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
    }
}
