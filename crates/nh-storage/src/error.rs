use thiserror::Error;

/// Result type alias for nh-storage operations
pub type Result<T> = std::result::Result<T, Error>;

/// Comprehensive error type for nh-storage
#[derive(Debug, Error)]
pub enum Error {
    /// SQLite database error
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Filesystem I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Database migration failed
    #[error("Migration failed: {reason}")]
    MigrationFailed { reason: String },

    /// Entry not found
    #[error("Not found: {entity} with {key}")]
    NotFound { entity: String, key: String },

    /// Duplicate entry
    #[error("Duplicate entry: {entity} with {key}")]
    Duplicate { entity: String, key: String },

    /// Cache entry not found
    #[error("Cache miss for {path}")]
    CacheMiss { path: String },

    /// Invalid path
    #[error("Invalid path: {path}")]
    InvalidPath { path: String },
}