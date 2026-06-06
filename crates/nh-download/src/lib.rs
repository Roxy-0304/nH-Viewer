//! # nh-download
//!
//! High-performance async download module for nh-viewer.
//!
//! This crate provides:
//! - **Priority queue** for managing download tasks
//! - **Concurrent workers** with configurable parallelism
//! - **Chunked downloads** with HTTP Range requests and resume support
//! - **Progress reporting** via trait-based callbacks
//! - **ZIP/CBZ archiving** with streaming writes
//! - **Cache integration** with nh-storage
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use nh_download::DownloadManager;
//! use nh_download::progress::NoopReporter;
//! use nh_download::queue::Priority;
//!
//! #[tokio::main]
//! async fn main() -> nh_download::error::Result<()> {
//!     let api_client = nh_api::NhClient::with_api_key(None).await?;
//!     let settings = nh_storage::settings::Settings::default_for_platform()?;
//!     let storage = nh_storage::Storage::init(settings).await?;
//!
//!     let mut manager = DownloadManager::new(
//!         api_client,
//!         storage,
//!         4, // concurrency
//!         NoopReporter,
//!     ).await?;
//!
//!     manager.download_gallery(12345, Priority::High).await?;
//!
//!     // Later, shut down gracefully
//!     manager.shutdown().await;
//!     Ok(())
//! }
//! ```

pub mod archiver;
pub mod chunked;
pub mod error;
pub mod progress;
pub mod queue;
pub mod worker;

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info};

use nh_api::GalleryEndpoints;

use crate::archiver::ArchiveOptions;
use crate::error::Result;
use crate::progress::{DynReporter, ProgressReporter};
use crate::queue::{DownloadQueue, Priority};
use crate::worker::WorkerPool;

/// Top-level download manager that orchestrates all download operations.
pub struct DownloadManager {
    api_client: nh_api::NhClient,
    storage: nh_storage::Storage,
    queue: DownloadQueue,
    worker_pool: Option<WorkerPool>,
    reporter: DynReporter,
    download_dir: PathBuf,
    concurrency: usize,
}

impl DownloadManager {
    /// Create a new download manager.
    ///
    /// # Arguments
    /// * `api_client` - nh-api client for fetching gallery metadata and CDN config
    /// * `storage` - nh-storage for cache integration
    /// * `concurrency` - Number of concurrent download workers
    /// * `reporter` - Progress reporter implementation
    pub async fn new(
        api_client: nh_api::NhClient,
        storage: nh_storage::Storage,
        concurrency: usize,
        reporter: impl ProgressReporter,
    ) -> Result<Self> {
        let queue = DownloadQueue::new();
        let reporter: DynReporter = Arc::new(reporter);
        let download_dir = storage.settings().image_cache_dir.clone();

        // Restore queue from persistence if available
        Self::restore_queue(&queue, &storage).await?;

        info!(concurrency, dir = %download_dir.display(), "download manager created");

        Ok(Self {
            api_client,
            storage,
            queue,
            worker_pool: None,
            reporter,
            download_dir,
            concurrency,
        })
    }

    /// Start the worker pool. Must be called before adding tasks.
    pub fn start_workers(&mut self) {
        if self.worker_pool.is_some() {
            return;
        }
        let pool = WorkerPool::new(
            self.concurrency,
            self.queue.clone(),
            Arc::clone(&self.reporter),
            self.download_dir.clone(),
        );
        self.worker_pool = Some(pool);
    }

    /// Download all pages of a gallery.
    ///
    /// Fetches gallery metadata from the API, adds all pages to the queue.
    pub async fn download_gallery(&mut self, gallery_id: u64, priority: Priority) -> Result<()> {
        info!(gallery_id, "downloading gallery");

        let gallery = self.api_client.get_gallery(gallery_id).await?;
        let cdn = self.api_client.cdn_config();

        // Round-robin CDN server assignment for load balancing
        let num_servers = cdn.image_servers.len().max(1);

        for page in 1..=gallery.num_pages {
            let ext = gallery
                .images
                .pages
                .get((page - 1) as usize)
                .map(|e| e.as_extension().to_string())
                .unwrap_or_else(|| "jpg".to_string());

            let server_index = (page as usize) % num_servers;

            self.queue
                .add_task(
                    gallery_id,
                    gallery.media_id.clone(),
                    page,
                    ext,
                    priority,
                    server_index,
                )
                .await;
        }

        // Ensure workers are running
        self.start_workers();

        // Persist queue state
        self.persist_queue().await?;

        info!(gallery_id, pages = gallery.num_pages, "gallery queued for download");
        Ok(())
    }

    /// Download a single page.
    pub async fn download_page(
        &mut self,
        gallery_id: u64,
        media_id: String,
        page: u32,
        ext: String,
        priority: Priority,
        server_index: usize,
    ) -> Result<()> {
        self.queue
            .add_task(gallery_id, media_id, page, ext, priority, server_index)
            .await;

        // Ensure workers are running
        self.start_workers();

        self.persist_queue().await?;
        Ok(())
    }

    /// Pause a single task.
    pub async fn pause(&self, task_id: u64) -> bool {
        self.queue.pause(task_id).await
    }

    /// Resume a single task.
    pub async fn resume(&self, task_id: u64) -> bool {
        self.queue.resume(task_id).await
    }

    /// Cancel a single task.
    pub async fn cancel(&self, task_id: u64) -> bool {
        self.queue.cancel(task_id).await
    }

    /// Pause all tasks.
    pub async fn pause_all(&self) {
        self.queue.pause_all().await;
    }

    /// Resume all tasks.
    pub async fn resume_all(&self) {
        self.queue.resume_all().await;
    }

    /// Cancel all tasks.
    pub async fn cancel_all(&self) {
        self.queue.cancel_all().await;
    }

    /// Get a snapshot of all tasks.
    pub async fn task_snapshot(&self) -> Vec<queue::DownloadTask> {
        self.queue.snapshot().await
    }

    /// Access the underlying storage.
    pub fn storage(&self) -> &nh_storage::Storage {
        &self.storage
    }

    /// Access the download queue.
    pub fn queue(&self) -> &DownloadQueue {
        &self.queue
    }

    /// Archive a downloaded gallery to ZIP/CBZ.
    pub async fn archive_gallery(
        &self,
        gallery_id: u64,
        output_path: &std::path::Path,
        options: Option<ArchiveOptions>,
    ) -> Result<()> {
        let gallery_dir = self.download_dir.join(gallery_id.to_string());
        let opts = options.unwrap_or_default();

        let entries = archiver::entries_from_dir(&gallery_dir, opts.format).await?;
        archiver::create_archive(&entries, output_path, &opts).await?;

        info!(
            gallery_id,
            output = %output_path.display(),
            "gallery archived"
        );
        Ok(())
    }

    /// Persist queue state to disk for crash recovery.
    async fn persist_queue(&self) -> Result<()> {
        let json = self.queue.to_json().await;
        let persist_path = self.download_dir.join(".queue_state.json");
        if let Some(parent) = persist_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&persist_path, &json).await?;
        debug!("queue state persisted");
        Ok(())
    }

    /// Restore queue state from disk.
    async fn restore_queue(queue: &DownloadQueue, storage: &nh_storage::Storage) -> Result<()> {
        let persist_path = storage
            .settings()
            .image_cache_dir
            .join(".queue_state.json");
        if persist_path.exists() {
            let json = tokio::fs::read_to_string(&persist_path).await?;
            queue.from_json(&json).await?;
            info!("queue state restored from disk");
        }
        Ok(())
    }

    /// Shut down the download manager gracefully.
    /// Persists queue state and waits for workers to finish.
    pub async fn shutdown(mut self) {
        // Persist queue state before shutdown
        let _ = self.persist_queue().await;

        // Signal workers to stop and wait
        if let Some(pool) = self.worker_pool.take() {
            pool.shutdown().await;
        }

        info!("download manager shut down");
    }
}