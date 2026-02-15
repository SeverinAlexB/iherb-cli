use crate::error::IherbError;
use std::path::PathBuf;

/// Resolves the Chrome binary path. Priority:
/// 1. User-configured path (from config)
/// 2. System-installed Chrome detection
/// 3. Previously downloaded Chrome for Testing
/// 4. Auto-download Chrome for Testing
pub async fn resolve_chrome(
    user_path: Option<&PathBuf>,
    data_dir: &PathBuf,
) -> Result<PathBuf, IherbError> {
    // 1. User-configured path
    if let Some(path) = user_path {
        if path.exists() {
            tracing::info!("Using user-configured browser: {}", path.display());
            return Ok(path.clone());
        }
        tracing::warn!(
            "User-configured browser path does not exist: {}",
            path.display()
        );
    }

    // 2. System-installed Chrome
    if let Some(path) = detect_system_chrome() {
        tracing::info!("Using system Chrome: {}", path.display());
        return Ok(path);
    }

    // 3. Previously downloaded Chrome
    let downloaded = downloaded_chrome_path(data_dir);
    if downloaded.exists() {
        tracing::info!("Using downloaded Chrome: {}", downloaded.display());
        return Ok(downloaded);
    }

    // 4. Auto-download
    tracing::info!("No Chrome found. Downloading Chrome for Testing...");
    let path = super::download::download_chrome(data_dir).await?;
    Ok(path)
}

fn detect_system_chrome() -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "macos") {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ]
    } else {
        vec![]
    };

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // Try `which` on unix
    #[cfg(unix)]
    {
        if let Ok(output) = std::process::Command::new("which")
            .arg("google-chrome")
            .output()
        {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    return Some(PathBuf::from(path_str));
                }
            }
        }
    }

    None
}

pub fn downloaded_chrome_path(data_dir: &PathBuf) -> PathBuf {
    let chrome_dir = data_dir.join("chrome");
    if cfg!(target_os = "macos") {
        chrome_dir
            .join("Google Chrome for Testing.app")
            .join("Contents")
            .join("MacOS")
            .join("Google Chrome for Testing")
    } else if cfg!(target_os = "windows") {
        chrome_dir.join("chrome.exe")
    } else {
        chrome_dir.join("chrome")
    }
}
