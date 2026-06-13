use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tokio::fs;

use crate::error::Result;

/// A cached file entry used for LRU tracking.
#[derive(Debug, Clone)]
struct CacheEntry {
    path: PathBuf,
    size: u64,
    /// Last access time as Unix timestamp (seconds)
    accessed_at: u64,
}

/// Thumbnail disk cache with LRU eviction.
///
/// Layout on disk:
/// ```text
/// {base_dir}/{gallery_id}/{page}.jpg
/// ```
///
/// When total cache size exceeds `max_size`, the least-recently-used files
/// are evicted automatically.
pub struct ThumbnailCache {
    base_dir: PathBuf,
    /// Maximum total cache size in bytes
    max_size: u64,
}

impl ThumbnailCache {
    /// Create a new `ThumbnailCache` rooted at `base_dir` with the given max size in bytes.
    pub fn new(base_dir: impl Into<PathBuf>, max_size: u64) -> Self {
        Self {
            base_dir: base_dir.into(),
            max_size,
        }
    }

    /// Root directory of this cache
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Maximum cache size in bytes
    pub const fn max_size(&self) -> u64 {
        self.max_size
    }

    /// Set a new maximum cache size
    pub fn set_max_size(&mut self, max_size: u64) {
        self.max_size = max_size;
    }

    /// Return the expected on-disk path for a cached thumbnail.
    fn cache_path(&self, gallery_id: u64, page: u32) -> PathBuf {
        self.base_dir
            .join(gallery_id.to_string())
            .join(format!("{}.jpg", page))
    }

    /// Read cached thumbnail bytes, returning `None` on cache miss.
    /// On hit, updates the file's modification time for LRU tracking.
    pub async fn get(&self, gallery_id: u64, page: u32) -> Result<Option<Vec<u8>>> {
        let path = self.cache_path(gallery_id, page);
        match fs::read(&path).await {
            Ok(data) => {
                // Touch the file to update mtime for LRU tracking.
                // tokio::fs has no set_modified, so use std::fs via spawn_blocking.
                let p = path.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    use std::fs::File;
                    use std::time::SystemTime;
                    if let Ok(f) = File::open(&p) {
                        let _ = f.set_times(
                            std::fs::FileTimes::new()
                                .set_accessed(SystemTime::now())
                                .set_modified(SystemTime::now()),
                        );
                    }
                })
                .await;
                Ok(Some(data))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Write a thumbnail into the cache.
    /// After writing, triggers eviction if the total size exceeds the limit.
    pub async fn put(&self, gallery_id: u64, page: u32, data: &[u8]) -> Result<PathBuf> {
        let path = self.cache_path(gallery_id, page);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await?;

        // Evict if over limit
        self.evict_if_needed().await?;

        Ok(path)
    }

    /// Check whether a cached thumbnail exists.
    pub async fn exists(&self, gallery_id: u64, page: u32) -> bool {
        let path = self.cache_path(gallery_id, page);
        fs::metadata(&path).await.is_ok()
    }

    /// Delete a single cached thumbnail.
    pub async fn delete(&self, gallery_id: u64, page: u32) -> Result<bool> {
        let path = self.cache_path(gallery_id, page);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Remove all cached thumbnails for a gallery.
    pub async fn clear_gallery(&self, gallery_id: u64) -> Result<()> {
        let dir = self.base_dir.join(gallery_id.to_string());
        if fs::metadata(&dir).await.is_ok() {
            fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// Scan the cache directory and return all cached files sorted by access time (oldest first).
    async fn scan_entries(&self) -> Result<Vec<CacheEntry>> {
        let mut entries = Vec::new();
        self.scan_dir_recursive(&self.base_dir, &mut entries)
            .await?;
        // Sort by accessed_at ascending (oldest = least recently used first)
        entries.sort_by_key(|e| e.accessed_at);
        Ok(entries)
    }

    /// Recursively scan a directory for cache entry files.
    fn scan_dir_recursive<'a>(
        &'a self,
        dir: &'a Path,
        entries: &'a mut Vec<CacheEntry>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut read_dir = fs::read_dir(dir).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let path = entry.path();
                let metadata = entry.metadata().await?;
                if metadata.is_dir() {
                    self.scan_dir_recursive(&path, entries).await?;
                } else if metadata.is_file() {
                    let accessed_at = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    entries.push(CacheEntry {
                        path,
                        size: metadata.len(),
                        accessed_at,
                    });
                }
            }
            Ok(())
        })
    }

    /// Evict least-recently-used files until total size is within `max_size`.
    async fn evict_if_needed(&self) -> Result<()> {
        let entries = self.scan_entries().await?;
        let total: u64 = entries.iter().map(|e| e.size).sum();

        if total <= self.max_size {
            return Ok(());
        }

        let mut remaining = total;
        // entries are sorted oldest-first, so we evict from the front
        for entry in &entries {
            if remaining <= self.max_size {
                break;
            }
            let _ = fs::remove_file(&entry.path).await;
            remaining = remaining.saturating_sub(entry.size);

            // Try to remove empty parent directory (gallery_id folder)
            if let Some(parent) = entry.path.parent() {
                if let Ok(mut dir) = fs::read_dir(parent).await {
                    if dir.next_entry().await.ok().flatten().is_none() {
                        let _ = fs::remove_dir(parent).await;
                    }
                }
            }
        }

        Ok(())
    }

    /// Calculate the current total cache size in bytes.
    pub async fn total_size(&self) -> Result<u64> {
        let entries = self.scan_entries().await?;
        Ok(entries.iter().map(|e| e.size).sum())
    }
}

impl std::fmt::Debug for ThumbnailCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailCache")
            .field("base_dir", &self.base_dir)
            .field("max_size", &self.max_size)
            .finish()
    }
}
