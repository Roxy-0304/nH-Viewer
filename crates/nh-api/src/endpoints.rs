use crate::error::Result;
use crate::types::{Gallery, PaginatedResponse};

/// Trait providing high-level API endpoint methods
///
/// This trait can be used to abstract over the API client for testing
/// or alternative implementations.
pub trait GalleryEndpoints {
    /// Get a single gallery by its ID
    fn get_gallery(&self, id: u64) -> impl std::future::Future<Output = Result<Gallery>> + Send;

    /// Search galleries by query string
    fn search(
        &self,
        query: &str,
        page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<Gallery>>> + Send;

    /// Get galleries filtered by tag ID
    fn tagged(
        &self,
        tag_id: u64,
        page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<Gallery>>> + Send;

    /// Get all galleries (index)
    fn all(
        &self,
        page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<Gallery>>> + Send;
}