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
//!         NoopReporter,
//!     ).await?;
//!
//!     manager.start(4); // 4 concurrent workers
//!     manager.submit_gallery_download(12345).await?;
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

use tracing::{debug, info, warn};

use nh_api::GalleryEndpoints;
use nh_storage::db::gallery_cache;

use crate::archiver::ArchiveOptions;
use crate::error::Result;
use crate::progress::{DynReporter, ProgressReporter};
use crate::queue::{DownloadQueue, Priority};
use crate::worker::WorkerPool;

/// Top-level download manager that orchestrates all download operations.
///
/// ## Lifecycle
///
/// 1. Create with [`DownloadManager::new`]
/// 2. Call [`start`](Self::start) to spin up worker coroutines
/// 3. Submit work with [`submit_gallery_download`](Self::submit_gallery_download)
///    or [`download_page`](Self::download_page)
/// 4. Optionally pause / resume / cancel individual tasks
/// 5. Call [`stop`](Self::stop) or [`shutdown`](Self::shutdown) to tear down
pub struct DownloadManager {
    api_client: nh_api::NhClient,
    storage: nh_storage::Storage,
    queue: Arc<DownloadQueue>,
    worker_pool: Option<WorkerPool>,
    reporter: DynReporter,
    download_dir: PathBuf,
    concurrency: usize,
}

impl DownloadManager {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new download manager.
    ///
    /// The manager is inert after construction — call [`start`](Self::start)
    /// to spin up worker coroutines before submitting work.
    ///
    /// # Arguments
    /// * `api_client` — nh-api client for fetching gallery metadata and CDN config
    /// * `storage`    — nh-storage for cache integration
    /// * `reporter`   — Progress reporter implementation
    pub async fn new(
        api_client: nh_api::NhClient,
        storage: nh_storage::Storage,
        reporter: impl ProgressReporter,
    ) -> Result<Self> {
        let queue = Arc::new(DownloadQueue::new());
        let reporter: DynReporter = Arc::new(reporter);
        let download_dir = storage.settings().image_cache_dir.clone();

        // Restore queue from persistence if available
        Self::restore_queue(&queue, &storage).await?;

        info!(dir = %download_dir.display(), "download manager created");

        Ok(Self {
            api_client,
            storage,
            queue,
            worker_pool: None,
            reporter,
            download_dir,
            concurrency: 0,
        })
    }

    // -----------------------------------------------------------------------
    // Lifecycle: start / stop
    // -----------------------------------------------------------------------

    /// Initialise control signals and spin up `concurrency` async worker
    /// coroutines.  Workers will automatically pick up tasks from the queue.
    ///
    /// Calling `start` when workers are already running is a no-op unless
    /// `concurrency` differs from the current value, in which case the pool
    /// is restarted with the new concurrency.
    pub fn start(&mut self, concurrency: usize) {
        if self.worker_pool.is_some() && self.concurrency == concurrency {
            debug!(concurrency, "workers already running, skipping");
            return;
        }

        // If concurrency changed, shut down old pool first
        if self.worker_pool.is_some() {
            info!("concurrency changed, restarting workers");
            // Take the pool so it is dropped (workers will observe shutdown via watch)
            self.worker_pool.take();
        }

        self.concurrency = concurrency;
        let proxy = self.api_client.build_reqwest_proxy().ok().flatten();
        let cdn_config = self.api_client.cdn_config().clone();
        let pool = WorkerPool::new(
            concurrency,
            (*self.queue).clone(),
            Arc::clone(&self.reporter),
            self.download_dir.clone(),
            proxy,
            cdn_config,
        );
        self.worker_pool = Some(pool);
        info!(concurrency, "worker pool started");
    }

    /// Gracefully stop all workers and persist the current queue snapshot
    /// to disk so work can be resumed after a restart.
    pub async fn stop(&mut self) {
        // Persist queue state before stopping
        if let Err(e) = self.persist_queue().await {
            warn!(error = %e, "failed to persist queue state during stop");
        }

        // Signal workers to stop and wait for them to finish
        if let Some(pool) = self.worker_pool.take() {
            pool.shutdown().await;
        }

        info!("download manager stopped");
    }

    /// Stop workers and consume the manager.
    ///
    /// Equivalent to `stop()` + drop.  Prefer this over `drop` alone to
    /// ensure queue persistence.
    pub async fn shutdown(mut self) {
        self.stop().await;
        info!("download manager shut down");
    }

    // -----------------------------------------------------------------------
    // Core business: submit_gallery_download
    // -----------------------------------------------------------------------

    /// Submit all pages of a gallery for download (cache-first strategy).
    ///
    /// **Pipeline:**
    /// 1. Check the local SQLite cache via `nh-storage` for the gallery.
    ///    On cache miss, fetch from the remote API through `nh-api` and
    ///    persist the raw JSON to the database.
    /// 2. Parse the `Gallery` metadata (`num_pages`, `media_id`, per-page
    ///    image types) and construct a download URL and local save path
    ///    for every page.
    /// 3. Enqueue each page as a separate [`DownloadTask`] into the
    ///    `DownloadQueue` (skipping pages whose final file already exists
    ///    on disk, and deduplicating against in-flight tasks).
    /// 4. Ensure the `WorkerPool` is running so workers can start
    ///    consuming tasks immediately.
    pub async fn submit_gallery_download(&self, gallery_id: u64) -> anyhow::Result<()> {
        info!(gallery_id, "submitting gallery download");

        // ---- Step 1: Cache-first gallery fetch ----
        let pool = self.storage.db().pool();

        let gallery: nh_api::GalleryDetailResponse = match gallery_cache::get_gallery_raw(
            pool, gallery_id,
        )
        .await?
        {
            Some(ref raw) if !raw.is_empty() => {
                match serde_json::from_str::<nh_api::GalleryDetailResponse>(raw) {
                    Ok(g) => {
                        debug!(gallery_id, "gallery loaded from cache");
                        g
                    }
                    Err(e) => {
                        warn!(gallery_id, error = %e, "cached JSON corrupt, re-fetching from API");
                        self.api_client.get_gallery(gallery_id, None).await?
                    }
                }
            }
            _ => {
                // Cache miss → fetch from remote API
                let g = self.api_client.get_gallery(gallery_id, None).await?;

                // Persist raw JSON to cache (best-effort)
                let raw = serde_json::to_string(&g).unwrap_or_default();
                if let Err(e) = gallery_cache::upsert_gallery(pool, gallery_id, &raw).await {
                    warn!(gallery_id, error = %e, "failed to cache gallery JSON");
                }

                g
            }
        };

        // ---- Step 2: Parse image list & construct per-page URLs/paths ----
        let cdn = self.api_client.cdn_config();
        let num_servers = cdn.image_servers.len().max(1);

        // Save path base: {download_dir}/{gallery_id}/
        let gallery_dir = self.download_dir.join(gallery_id.to_string());

        info!(
            gallery_id,
            media_id = %gallery.media_id,
            pages = gallery.num_pages,
            "enqueuing pages"
        );

        // ---- Step 3: Enqueue every page ----
        let mut skipped = 0u32;
        for page in 1..=gallery.num_pages {
            let page_info = gallery.pages.get((page - 1) as usize);
            let ext = page_info
                .map(|p| p.path.rsplit('.').next().unwrap_or("jpg").to_string())
                .unwrap_or_else(|| "jpg".to_string());

            let path = page_info
                .map(|p| p.path.clone())
                .unwrap_or_else(|| format!("/galleries/{}/{}.{}", gallery.media_id, page, ext));

            // Skip pages whose final file already exists on disk.
            let dest = gallery_dir.join(format!("{}.{}", page, ext));
            if let Ok(meta) = tokio::fs::metadata(&dest).await {
                if meta.len() > 0 {
                    debug!(gallery_id, page, "file already exists, skipping enqueue");
                    skipped += 1;
                    continue;
                }
            }

            let server_index = (page as usize) % num_servers;

            self.queue
                .add_task(
                    gallery_id,
                    gallery.media_id.clone(),
                    page,
                    ext,
                    Priority::Medium,
                    server_index,
                    path,
                )
                .await;
        }

        // Persist queue snapshot for crash recovery
        let _ = self.persist_queue().await;

        info!(
            gallery_id,
            pages = gallery.num_pages,
            skipped,
            "gallery download submitted"
        );

        debug!("queue now has tasks; ensure start(concurrency) has been called");

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Single-page download
    // -----------------------------------------------------------------------

    /// Download a single page.
    pub async fn download_page(
        &self,
        gallery_id: u64,
        media_id: String,
        page: u32,
        ext: String,
        priority: Priority,
        server_index: usize,
        path: String,
    ) -> Result<()> {
        self.queue
            .add_task(
                gallery_id,
                media_id,
                page,
                ext,
                priority,
                server_index,
                path,
            )
            .await;

        self.persist_queue().await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Task management
    // -----------------------------------------------------------------------

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

    /// Cancel all tasks and remove them from the queue entirely.
    pub async fn cancel_all_and_clear(&self) {
        self.queue.clear().await;
    }

    /// Get a snapshot of all tasks.
    pub async fn task_snapshot(&self) -> Vec<queue::DownloadTask> {
        self.queue.snapshot().await
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Access the underlying storage.
    pub fn storage(&self) -> &nh_storage::Storage {
        &self.storage
    }

    /// Access the download queue.
    pub fn queue(&self) -> &DownloadQueue {
        &self.queue
    }

    // -----------------------------------------------------------------------
    // Archiving
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Queue persistence (private)
    // -----------------------------------------------------------------------

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
        let persist_path = storage.settings().image_cache_dir.join(".queue_state.json");
        if persist_path.exists() {
            let json = tokio::fs::read_to_string(&persist_path).await?;
            queue.from_json(&json).await?;
            info!("queue state restored from disk");
        }
        Ok(())
    }
}
