use thiserror::Error;

/// Result type alias for nh-download operations
pub type Result<T> = std::result::Result<T, Error>;

/// Comprehensive error type for nh-download
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP/network error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Filesystem I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// nh-api error
    #[error("API error: {0}")]
    Api(#[from] nh_api::Error),

    /// nh-storage error
    #[error("Storage error: {0}")]
    Storage(#[from] nh_storage::error::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// ZIP archive error
    #[error("ZIP error: {0}")]
    Zip(String),

    /// Task was cancelled by user
    #[error("Task {task_id} cancelled")]
    Cancelled { task_id: u64 },

    /// Task is paused
    #[error("Task {task_id} is paused")]
    Paused { task_id: u64 },

    /// A single chunk download failed after retries
    #[error("Chunk download failed for {url} after {retries} retries: {reason}")]
    ChunkFailed {
        url: String,
        retries: u32,
        reason: String,
    },

    /// Server does not support Range requests
    #[error("Range requests not supported by server")]
    RangeNotSupported,

    /// All worker tasks have been shut down
    #[error("Worker pool shut down")]
    WorkerPoolShutdown,
}
