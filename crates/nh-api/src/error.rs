use thiserror::Error;

/// Result type alias for nh-api operations
pub type Result<T> = std::result::Result<T, Error>;

/// Comprehensive error type for nh-api
#[derive(Debug, Error)]
pub enum Error {
    /// Network or HTTP client error
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialization error
    #[error("JSON deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP error response with status code
    #[error("HTTP error {status}: {body}")]
    HttpError { status: u16, body: String },

    /// Invalid or missing API key
    #[error("Invalid or missing API key")]
    InvalidApiKey,

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after:?}")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },

    /// Invalid page number
    #[error("Invalid page number: {page}")]
    InvalidPage { page: u32 },

    /// Invalid gallery ID
    #[error("Invalid gallery ID: {id}")]
    InvalidGalleryId { id: u64 },

    /// Invalid tag ID
    #[error("Invalid tag ID: {id}")]
    InvalidTagId { id: u64 },

    /// Invalid media ID
    #[error("Invalid media ID: {media_id}")]
    InvalidMediaId { media_id: String },

    /// Invalid page index for image
    #[error("Invalid page index: {page} (max: {max})")]
    InvalidPageNumber { page: u32, max: u32 },

    /// Image extension not found
    #[error("Image extension not found for page {page}")]
    ImageExtensionNotFound { page: u32 },

    /// Retry failed after maximum attempts
    #[error("Retry failed after {attempts} attempts")]
    RetryFailed { attempts: u32 },

    /// CDN configuration fetch failed
    #[error("Failed to fetch CDN config: {reason}")]
    CdnConfigFetch { reason: String },

    /// Invalid server index for CDN
    #[error("Invalid server index: {index}, available: {available}")]
    InvalidServerIndex { index: usize, available: usize },
}
