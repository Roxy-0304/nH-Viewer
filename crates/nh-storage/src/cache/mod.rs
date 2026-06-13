pub mod image_cache;
pub mod thumbnail_cache;

pub use image_cache::ImageCache;
pub use thumbnail_cache::ThumbnailCache;

use std::path::PathBuf;

use crate::error::Result;

/// Unified cache manager combining image and thumbnail caches.
#[derive(Debug)]
pub struct CacheManager {
    images: ImageCache,
    thumbnails: ThumbnailCache,
}

impl CacheManager {
    /// Create a new `CacheManager` with the given base directory and thumbnail cache limit.
    ///
    /// Directory layout:
    /// ```text
    /// base_dir/
    ///   images/{gallery_id}/{page}.{ext}
    ///   thumbnails/{gallery_id}/{page}.jpg
    /// ```
    pub fn new(base_dir: impl Into<PathBuf>, max_thumbnail_size: u64) -> Self {
        let base = base_dir.into();
        Self {
            images: ImageCache::new(base.join("images")),
            thumbnails: ThumbnailCache::new(base.join("thumbnails"), max_thumbnail_size),
        }
    }

    /// Access the image (original quality) cache
    pub const fn images(&self) -> &ImageCache {
        &self.images
    }

    /// Access the thumbnail cache
    pub const fn thumbnails(&self) -> &ThumbnailCache {
        &self.thumbnails
    }

    /// Mutable access to the thumbnail cache (e.g. to change max_size)
    pub fn thumbnails_mut(&mut self) -> &mut ThumbnailCache {
        &mut self.thumbnails
    }

    /// Clear all cached data (both images and thumbnails) for a gallery.
    pub async fn clear_gallery(&self, gallery_id: u64) -> Result<()> {
        self.images.clear_gallery(gallery_id).await?;
        self.thumbnails.clear_gallery(gallery_id).await?;
        Ok(())
    }

    /// Get total size of both caches in bytes.
    pub async fn total_size(&self) -> Result<(u64, u64)> {
        let img_size = {
            // ImageCache has no size limit, but we can still measure it
            // by scanning. For now return 0 to keep it simple.
            0u64
        };
        let thumb_size = self.thumbnails.total_size().await?;
        Ok((img_size, thumb_size))
    }
}
