use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::error::{Error, Result};

/// Default maximum thumbnail cache size: 500 MB
const DEFAULT_MAX_THUMBNAIL_CACHE: u64 = 500 * 1024 * 1024;

/// Application settings persisted as a JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Root data directory for the application
    pub data_dir: PathBuf,

    /// Directory for image (original) cache
    pub image_cache_dir: PathBuf,

    /// Directory for thumbnail cache
    pub thumbnail_cache_dir: PathBuf,

    /// Path to the SQLite database file
    pub db_path: PathBuf,

    /// Optional nhentai API key
    #[serde(default)]
    pub api_key: Option<String>,

    /// Maximum thumbnail cache size in bytes
    #[serde(default = "default_max_thumbnail_cache")]
    pub max_thumbnail_cache_size: u64,
}

const fn default_max_thumbnail_cache() -> u64 {
    DEFAULT_MAX_THUMBNAIL_CACHE
}

impl Settings {
    /// Create `Settings` with platform-appropriate defaults.
    ///
    /// - **Linux**: `~/.local/share/nh-viewer/`
    /// - **Windows**: `%APPDATA%/nh-viewer/`
    /// - **macOS**: `~/Library/Application Support/nh-viewer/`
    pub fn default_for_platform() -> Result<Self> {
        let base = dirs::data_dir().ok_or_else(|| Error::InvalidPath {
            path: "Could not determine platform data directory".to_string(),
        })?;
        let data_dir = base.join("nh-viewer");
        Ok(Self::from_data_dir(data_dir))
    }

    /// Build settings from a given data directory.
    /// All other paths are derived relative to `data_dir`.
    pub fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            image_cache_dir: data_dir.join("cache").join("images"),
            thumbnail_cache_dir: data_dir.join("cache").join("thumbnails"),
            db_path: data_dir.join("data.db"),
            data_dir,
            api_key: None,
            max_thumbnail_cache_size: default_max_thumbnail_cache(),
        }
    }

    /// Return the expected path for the settings JSON file.
    pub fn config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// Load settings from the default platform location.
    /// If the file does not exist, returns `Ok(None)`.
    pub async fn load_default() -> Result<Option<Self>> {
        let base = dirs::data_dir().ok_or_else(|| Error::InvalidPath {
            path: "Could not determine platform data directory".to_string(),
        })?;
        let data_dir = base.join("nh-viewer");
        let path = Self::config_path(&data_dir);
        Self::load(&path).await
    }

    /// Load settings from a specific JSON file path.
    /// Returns `Ok(None)` if the file does not exist.
    pub async fn load(path: &Path) -> Result<Option<Self>> {
        match fs::read_to_string(path).await {
            Ok(json) => {
                let settings: Self = serde_json::from_str(&json)?;
                Ok(Some(settings))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save settings to the default platform location.
    pub async fn save_default(&self) -> Result<()> {
        let path = Self::config_path(&self.data_dir);
        self.save(&path).await
    }

    /// Save settings to a specific JSON file path.
    /// Creates parent directories if they don't exist.
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        Ok(())
    }

    /// Ensure all configured directories exist.
    pub async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.data_dir).await?;
        fs::create_dir_all(&self.image_cache_dir).await?;
        fs::create_dir_all(&self.thumbnail_cache_dir).await?;
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }
}
