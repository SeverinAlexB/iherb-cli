use crate::config::AppConfig;
use crate::error::IherbError;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

const STEALTH_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const STEALTH_ARGS: &[&str] = &[
    "--disable-blink-features=AutomationControlled",
    "--disable-features=IsolateOrigins,site-per-process",
    "--disable-site-isolation-trials",
    "--disable-web-security",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-default-apps",
    "--disable-extensions",
    "--disable-popup-blocking",
    "--disable-translate",
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
    "--window-size=1920,1080",
];

pub struct BrowserSession {
    browser: Arc<Mutex<Browser>>,
    _handle: tokio::task::JoinHandle<()>,
    user_data_dir: PathBuf,
}

impl BrowserSession {
    pub async fn launch(chrome_path: PathBuf, config: &AppConfig) -> Result<Self, IherbError> {
        // Create a unique user data directory to avoid SingletonLock conflicts
        // when multiple instances run concurrently or after a stale lock is left behind.
        let user_data_dir = std::env::temp_dir().join(format!(
            "iherb-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::create_dir_all(&user_data_dir).map_err(|e| {
            IherbError::BrowserLaunch(format!(
                "Failed to create user data dir {}: {}",
                user_data_dir.display(),
                e
            ))
        })?;

        let mut builder = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .user_data_dir(user_data_dir.clone())
            .arg(format!("--user-agent={}", STEALTH_USER_AGENT))
            .viewport(None);

        for arg in STEALTH_ARGS {
            builder = builder.arg(*arg);
        }

        if !config.debug {
            builder = builder.arg("--headless=new");
        }

        let browser_config = builder
            .build()
            .map_err(|e| IherbError::BrowserLaunch(format!("{}", e)))?;

        let (browser, mut handler) = Browser::launch(browser_config)
            .await
            .map_err(|e| IherbError::BrowserLaunch(format!("{}", e)))?;

        let handle = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                tracing::trace!("Browser event: {:?}", event);
            }
        });

        Ok(BrowserSession {
            browser: Arc::new(Mutex::new(browser)),
            _handle: handle,
            user_data_dir,
        })
    }

    pub async fn new_page(&self) -> Result<Page, IherbError> {
        let browser = self.browser.lock().await;
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| IherbError::BrowserLaunch(format!("Failed to create page: {}", e)))?;

        // Stealth: override navigator.webdriver and other detection vectors
        let _ = page
            .evaluate(
                r#"
                Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
                Object.defineProperty(navigator, 'languages', { get: () => ['en-US', 'en'] });
                Object.defineProperty(navigator, 'plugins', { get: () => [1, 2, 3, 4, 5] });

                // Override chrome.runtime to prevent detection
                window.chrome = { runtime: {} };

                // Override permissions query
                const originalQuery = window.navigator.permissions.query;
                window.navigator.permissions.query = (parameters) => (
                    parameters.name === 'notifications' ?
                    Promise.resolve({ state: Notification.permission }) :
                    originalQuery(parameters)
                );
                "#,
            )
            .await;

        Ok(page)
    }

    pub async fn close(self) -> Result<(), IherbError> {
        let mut browser = self.browser.lock().await;
        browser
            .close()
            .await
            .map_err(|e| IherbError::BrowserLaunch(format!("Failed to close browser: {}", e)))?;

        // Drop the browser handle so Chrome subprocesses can fully exit
        drop(browser);

        // Give Chrome subprocesses time to release file locks
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Clean up the temporary user data directory with retry
        if self.user_data_dir.exists() {
            for attempt in 1..=3 {
                match std::fs::remove_dir_all(&self.user_data_dir) {
                    Ok(_) => break,
                    Err(e) if attempt < 3 => {
                        tracing::debug!(
                            "Cleanup attempt {}/3 for {}: {}, retrying...",
                            attempt,
                            self.user_data_dir.display(),
                            e
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                    Err(_) => {
                        // Final attempt failed — silently ignore. The OS will clean /tmp.
                        tracing::debug!(
                            "Could not clean up temp dir {}, will be cleaned by OS",
                            self.user_data_dir.display()
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
