use crate::cli::SortOrder;
use crate::error::IherbError;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct Cache {
    dir: PathBuf,
    read_enabled: bool,
}

const PRODUCT_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const SEARCH_TTL: Duration = Duration::from_secs(60 * 60); // 1 hour

impl Cache {
    /// Create a cache. When `no_cache` is true, reads are skipped but writes still happen.
    pub fn new(cache_dir: PathBuf, no_cache: bool) -> Self {
        Self {
            dir: cache_dir,
            read_enabled: !no_cache,
        }
    }

    pub fn get_product<T: DeserializeOwned>(&self, product_id: &str) -> Option<T> {
        if !self.read_enabled {
            return None;
        }
        let path = self.dir.join(format!("product_{}.json", product_id));
        self.read_cached(&path, PRODUCT_TTL)
    }

    pub fn set_product<T: Serialize>(&self, product_id: &str, data: &T) -> Result<(), IherbError> {
        let path = self.dir.join(format!("product_{}.json", product_id));
        self.write_cached(&path, data)
    }

    pub fn get_search<T: DeserializeOwned>(
        &self,
        query: &str,
        sort: SortOrder,
        category: Option<&str>,
    ) -> Option<T> {
        if !self.read_enabled {
            return None;
        }
        let key = self.search_key(query, sort, category);
        let path = self.dir.join(format!("search_{}.json", key));
        self.read_cached(&path, SEARCH_TTL)
    }

    pub fn set_search<T: Serialize>(
        &self,
        query: &str,
        sort: SortOrder,
        category: Option<&str>,
        data: &T,
    ) -> Result<(), IherbError> {
        let key = self.search_key(query, sort, category);
        let path = self.dir.join(format!("search_{}.json", key));
        self.write_cached(&path, data)
    }

    fn search_key(&self, query: &str, sort: SortOrder, category: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query.as_bytes());
        hasher.update(sort.as_cache_key().as_bytes());
        if let Some(cat) = category {
            hasher.update(cat.as_bytes());
        }
        let result = hasher.finalize();
        hex::encode(&result[..8]) // 16 hex chars
    }

    fn read_cached<T: DeserializeOwned>(&self, path: &Path, ttl: Duration) -> Option<T> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > ttl {
            tracing::debug!("Cache expired for {}", path.display());
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&content) {
            Ok(data) => {
                tracing::info!("Cache hit for {}", path.display());
                Some(data)
            }
            Err(e) => {
                tracing::warn!("Cache parse error for {}: {}", path.display(), e);
                None
            }
        }
    }

    fn write_cached<T: Serialize>(&self, path: &Path, data: &T) -> Result<(), IherbError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| IherbError::Cache(format!("Failed to create cache dir: {}", e)))?;
        let content = serde_json::to_string_pretty(data)?;
        std::fs::write(path, content)
            .map_err(|e| IherbError::Cache(format!("Failed to write cache: {}", e)))?;
        tracing::debug!("Cached to {}", path.display());
        Ok(())
    }
}
