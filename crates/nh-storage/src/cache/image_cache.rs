use std::path::{Path, PathBuf};

use tokio::fs;

use crate::error::Result;

/// Raw image (original quality) disk cache.
///
/// Layout on disk:
/// ```text
/// {base_dir}/{gallery_id}/{page}.{ext}
/// ```
pub struct ImageCache {
    base_dir: PathBuf,
}

impl ImageCache {
    /// Create a new `ImageCache` rooted at `base_dir`.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Root directory of this cache
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Return the expected on-disk path for a cached page image.
    fn cache_path(&self, gallery_id: u64, page: u32, ext: &str) -> PathBuf {
        self.base_dir
            .join(gallery_id.to_string())
            .join(format!("{}.{}", page, ext))
    }

    /// Read cached image bytes, returning `None` on cache miss.
    pub async fn get(&self, gallery_id: u64, page: u32, ext: &str) -> Result<Option<Vec<u8>>> {
        let path = self.cache_path(gallery_id, page, ext);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Write image bytes into the cache.
    pub async fn put(
        &self,
        gallery_id: u64,
        page: u32,
        ext: &str,
        data: &[u8],
    ) -> Result<PathBuf> {
        let path = self.cache_path(gallery_id, page, ext);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await?;
        Ok(path)
    }

    /// Check whether a cached file exists.
    pub async fn exists(&self, gallery_id: u64, page: u32, ext: &str) -> bool {
        let path = self.cache_path(gallery_id, page, ext);
        fs::metadata(&path).await.is_ok()
    }

    /// Delete a single cached page.
    pub async fn delete(&self, gallery_id: u64, page: u32, ext: &str) -> Result<bool> {
        let path = self.cache_path(gallery_id, page, ext);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Remove all cached pages for a gallery.
    pub async fn clear_gallery(&self, gallery_id: u64) -> Result<()> {
        let dir = self.base_dir.join(gallery_id.to_string());
        if fs::metadata(&dir).await.is_ok() {
            fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCache")
            .field("base_dir", &self.base_dir)
            .finish()
    }
}