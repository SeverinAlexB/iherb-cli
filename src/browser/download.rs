use crate::error::IherbError;
use std::io::Read;
use std::path::PathBuf;

const CHROME_VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

pub async fn download_chrome(data_dir: &PathBuf) -> Result<PathBuf, IherbError> {
    let chrome_dir = data_dir.join("chrome");
    std::fs::create_dir_all(&chrome_dir)
        .map_err(|e| IherbError::ChromeDownload(format!("Failed to create dir: {}", e)))?;

    eprintln!("Fetching Chrome for Testing download URL...");
    let download_url = get_download_url().await?;

    eprintln!("Downloading Chrome for Testing...");
    let response = reqwest::get(&download_url)
        .await
        .map_err(|e| IherbError::ChromeDownload(format!("Download failed: {}", e)))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| IherbError::ChromeDownload(format!("Failed to read response: {}", e)))?;

    eprintln!("Extracting Chrome...");
    extract_zip(&bytes, &chrome_dir)?;

    let binary = super::resolve::downloaded_chrome_path(data_dir);
    if !binary.exists() {
        return Err(IherbError::ChromeDownload(format!(
            "Chrome binary not found after extraction at: {}",
            binary.display()
        )));
    }

    // Make executable on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&binary)
            .map_err(|e| IherbError::ChromeDownload(format!("Failed to read permissions: {}", e)))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms)
            .map_err(|e| IherbError::ChromeDownload(format!("Failed to set permissions: {}", e)))?;
    }

    eprintln!("Chrome for Testing installed at: {}", binary.display());
    Ok(binary)
}

async fn get_download_url() -> Result<String, IherbError> {
    let resp: serde_json::Value = reqwest::get(CHROME_VERSIONS_URL)
        .await
        .map_err(|e| IherbError::ChromeDownload(format!("Failed to fetch versions: {}", e)))?
        .json()
        .await
        .map_err(|e| IherbError::ChromeDownload(format!("Failed to parse versions: {}", e)))?;

    let platform = get_platform();

    let url = resp["channels"]["Stable"]["downloads"]["chrome"]
        .as_array()
        .and_then(|downloads| {
            downloads
                .iter()
                .find(|d| d["platform"].as_str() == Some(platform))
                .and_then(|d| d["url"].as_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| {
            IherbError::ChromeDownload(format!("No download found for platform: {}", platform))
        })?;

    Ok(url)
}

fn get_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        }
    } else if cfg!(target_os = "linux") {
        "linux64"
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "win64"
        } else {
            "win32"
        }
    } else {
        "linux64"
    }
}

fn extract_zip(data: &[u8], dest: &PathBuf) -> Result<(), IherbError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| IherbError::ChromeDownload(format!("Failed to open zip: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| IherbError::ChromeDownload(format!("Failed to read zip entry: {}", e)))?;

        // Strip the top-level directory from the zip (e.g., "chrome-mac-arm64/...")
        let name = file.name().to_string();
        let stripped = match name.find('/') {
            Some(idx) => &name[idx + 1..],
            None => continue,
        };

        if stripped.is_empty() {
            continue;
        }

        let out_path = dest.join(stripped);

        if file.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| IherbError::ChromeDownload(format!("Failed to create dir: {}", e)))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    IherbError::ChromeDownload(format!("Failed to create parent dir: {}", e))
                })?;
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| {
                IherbError::ChromeDownload(format!("Failed to read file from zip: {}", e))
            })?;
            std::fs::write(&out_path, &buf)
                .map_err(|e| IherbError::ChromeDownload(format!("Failed to write file: {}", e)))?;

            // Preserve executable permission on unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    let perms = std::fs::Permissions::from_mode(mode);
                    let _ = std::fs::set_permissions(&out_path, perms);
                }
            }
        }
    }

    Ok(())
}
