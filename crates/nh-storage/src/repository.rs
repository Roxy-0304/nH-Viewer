//! `GalleryRepository` trait — abstracts gallery data access.
//!
//! Implementations can back this with a local SQLite cache, a remote API, or both.

use crate::db::gallery_cache::{CachedGallery, GalleryPreview, SearchHistoryItem};
use crate::error::Error;

/// Errors specific to repository operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("storage error: {0}")]
    Storage(#[from] Error),
    #[error("not found: {entity} {key}")]
    NotFound { entity: String, key: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("other: {0}")]
    Other(String),
}

/// Abstract gallery data access.
///
/// `nh-storage` provides a local (SQLite + filesystem) implementation.
/// A network-backed implementation can be layered on top via `nh-api`.
#[async_trait::async_trait]
pub trait GalleryRepository: Send + Sync {
    /// Fetch a gallery by id. Returns `None` if not cached.
    async fn get_gallery(&self, id: u64) -> Result<Option<CachedGallery>, RepositoryError>;

    /// Search galleries by title (local LIKE search or remote API).
    async fn search_galleries(
        &self,
        query: &str,
        page: u32,
    ) -> Result<Vec<GalleryPreview>, RepositoryError>;

    /// Persist a gallery (metadata + optional raw JSON).
    async fn save_gallery(&self, gallery: &CachedGallery) -> Result<(), RepositoryError>;

    /// Record a search query in history.
    async fn record_search(&self, query: &str, result_count: u32) -> Result<(), RepositoryError>;

    /// List recent search history.
    async fn list_search_history(
        &self,
        limit: u32,
    ) -> Result<Vec<SearchHistoryItem>, RepositoryError>;

    /// Delete a cached gallery.
    async fn delete_gallery(&self, id: u64) -> Result<bool, RepositoryError>;
}
