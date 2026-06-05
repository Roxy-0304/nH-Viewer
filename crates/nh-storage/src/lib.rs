//! # nh-storage
//!
//! Local persistence layer for nh-viewer.
//!
//! This crate provides:
//! - **SQLite database** for browsing history and favorites
//! - **Disk caches** for original images and thumbnails (with LRU eviction)
//! - **Settings management** with JSON persistence
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use nh_storage::Storage;
//!
//! #[tokio::main]
//! async fn main() -> nh_storage::error::Result<()> {
//!     let settings = nh_storage::settings::Settings::default_for_platform()?;
//!     let storage = Storage::init(settings).await?;
//!     
//!     // Use storage.db(), storage.cache(), storage.settings()
//!     Ok(())
//! }
//! ```

pub mod cache;
pub mod db;
pub mod error;
pub mod settings;

use tracing::info;

use crate::cache::CacheManager;
use crate::db::Database;
use crate::error::Result;
use crate::settings::Settings;

/// Top-level storage handle combining database, caches, and settings.
#[derive(Debug)]
pub struct Storage {
    db: Database,
    cache: CacheManager,
    settings: Settings,
}

impl Storage {
    /// Initialize the storage layer with the given settings.
    ///
    /// This will:
    /// 1. Create all required directories
    /// 2. Open (or create) the SQLite database and run migrations
    /// 3. Initialize the image and thumbnail caches
    pub async fn init(settings: Settings) -> Result<Self> {
        info!("Initializing storage at {}", settings.data_dir.display());

        // Ensure directories exist
        settings.ensure_dirs().await?;

        // Open database
        let db = Database::open(&settings.db_path)?;

        // Create cache manager
        let cache = CacheManager::new(
            &settings.data_dir.join("cache"),
            settings.max_thumbnail_cache_size,
        );

        Ok(Self {
            db,
            cache,
            settings,
        })
    }

    /// Access the database
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Access the cache manager
    pub fn cache(&self) -> &CacheManager {
        &self.cache
    }

    /// Mutable access to the cache manager
    pub fn cache_mut(&mut self) -> &mut CacheManager {
        &mut self.cache
    }

    /// Access the current settings
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Mutable access to settings (remember to call `save()` after changes)
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Update settings and persist them to disk.
    pub async fn update_settings(&mut self, new_settings: Settings) -> Result<()> {
        self.settings = new_settings;
        self.settings.save_default().await?;
        Ok(())
    }
}