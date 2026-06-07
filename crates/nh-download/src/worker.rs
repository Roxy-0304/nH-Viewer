use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use nh_api::client::BROWSER_USER_AGENT;
use nh_api::image_url::build_image_url;
use nh_api::CdnConfig;

use crate::chunked::ChunkDownloader;
use crate::progress::DynReporter;
use crate::queue::DownloadQueue;

/// Default stale-lease timeout: 5 minutes.
const STALE_LEASE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Watchdog check interval: 30 seconds.
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(30);

/// Worker pool that runs concurrent download workers.
///
/// Each worker maintains its own HTTP client for connection pooling efficiency.
/// A watchdog task periodically reaps stale leases and detects dead workers.
pub struct WorkerPool {
    workers: Vec<tokio::task::JoinHandle<()>>,
    watchdog: tokio::task::JoinHandle<()>,
    shutdown_tx: watch::Sender<bool>,
}

impl WorkerPool {
    /// Create and start a new worker pool.
    ///
    /// # Arguments
    /// * `concurrency` - Number of concurrent download workers
    /// * `queue` - Shared download queue
    /// * `reporter` - Progress reporter
    /// * `download_dir` - Base directory for downloads
    pub fn new(
        concurrency: usize,
        queue: DownloadQueue,
        reporter: DynReporter,
        download_dir: std::path::PathBuf,
        proxy: Option<reqwest::Proxy>,
        cdn_config: CdnConfig,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut workers = Vec::with_capacity(concurrency);

        for worker_id in 0..concurrency {
            let q = queue.clone();
            let reporter = Arc::clone(&reporter);
            let mut rx = shutdown_rx.clone();
            let dir = download_dir.clone();
            let proxy = proxy.clone();

            // Each worker gets its own reqwest Client (independent connection pool)
            let mut builder = Client::builder()
                .user_agent(BROWSER_USER_AGENT)
                .timeout(std::time::Duration::from_secs(120));
            if let Some(p) = proxy {
                builder = builder.proxy(p);
            }
            let client = builder.build().expect("failed to build worker HTTP client");

            let downloader = ChunkDownloader::new(client);
            let cdn = cdn_config.clone();

            let handle = tokio::spawn(async move {
                info!(worker_id, "worker started");
                loop {
                    // Check for shutdown signal
                    if *rx.borrow() {
                        info!(worker_id, "worker shutting down");
                        break;
                    }

                    // Try to get next task, but also listen for shutdown
                    let task = tokio::select! {
                        _ = rx.changed() => {
                            if *rx.borrow() {
                                info!(worker_id, "worker shutting down");
                                break;
                            }
                            continue;
                        }
                        task = q.next() => task,
                    };

                    // next() only returns None if the queue is destroyed;
                    // treat it as a shutdown signal.
                    let task = match task {
                        Some(t) => t,
                        None => {
                            info!(worker_id, "queue closed, worker exiting");
                            break;
                        }
                    };

                    info!(
                        worker_id,
                        task_id = task.id,
                        gallery_id = task.gallery_id,
                        page = task.page,
                        "starting download"
                    );

                    // Build the destination path: {download_dir}/{gallery_id}/{page}.{ext}
                    let dest_path = dir
                        .join(task.gallery_id.to_string())
                        .join(format!("{}.{}", task.page, task.ext));

                    // Check if file already exists (cache hit)
                    if dest_path.exists() {
                        debug!(
                            worker_id,
                            task_id = task.id,
                            "file already exists, skipping"
                        );
                        q.complete(task.id).await;
                        reporter.on_complete(task.id);
                        continue;
                    }

                    // Try all CDN servers before giving up
                    let num_servers = cdn.image_servers.len().max(1);
                    let reporter_ref: &dyn crate::progress::ProgressReporter = &*reporter;
                    let mut last_err = String::new();
                    let mut succeeded = false;

                    for attempt in 0..num_servers {
                        let server_index = (task.server_index + attempt) % num_servers;
                        let url = build_image_url(&cdn, &task.path, server_index);

                        if attempt > 0 {
                            debug!(
                                worker_id,
                                task_id = task.id,
                                attempt,
                                server_index,
                                "retrying with different CDN server"
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }

                        match downloader
                            .download(&url, &dest_path, reporter_ref, task.id)
                            .await
                        {
                            Ok(bytes) => {
                                debug!(
                                    worker_id,
                                    task_id = task.id,
                                    bytes,
                                    server_index,
                                    "download succeeded"
                                );
                                q.complete(task.id).await;
                                succeeded = true;
                                break;
                            }
                            Err(e) => {
                                warn!(
                                    worker_id,
                                    task_id = task.id,
                                    server_index,
                                    error = %e,
                                    "download failed on CDN server, will try next"
                                );
                                last_err = e.to_string();
                            }
                        }
                    }

                    if !succeeded {
                        error!(
                            worker_id,
                            task_id = task.id,
                            servers_tried = num_servers,
                            error = %last_err,
                            "download failed on all CDN servers"
                        );
                        q.fail(task.id, format!("All {} CDN servers failed. Last error: {}", num_servers, last_err)).await;
                        reporter.on_error(task.id, &last_err);
                    }
                }
                info!(worker_id, "worker stopped");
            });

            workers.push(handle);
        }

        // ---- Watchdog task: reaps stale leases periodically ----
        let watchdog_queue = queue.clone();
        let mut watchdog_rx = shutdown_rx.clone();
        let watchdog = tokio::spawn(async move {
            info!("watchdog started");
            loop {
                tokio::select! {
                    _ = watchdog_rx.changed() => {
                        if *watchdog_rx.borrow() {
                            info!("watchdog shutting down");
                            break;
                        }
                    }
                    _ = tokio::time::sleep(WATCHDOG_INTERVAL) => {}
                }
                watchdog_queue.reap_stale_leases(STALE_LEASE_TIMEOUT).await;
            }
            info!("watchdog stopped");
        });

        info!(concurrency, "worker pool started");

        Self {
            workers,
            watchdog,
            shutdown_tx,
        }
    }

    /// Signal all workers to shut down and wait for them to finish.
    ///
    /// Also checks each worker for panics and logs warnings.
    pub async fn shutdown(self) {
        info!("shutting down worker pool");
        let _ = self.shutdown_tx.send(true);

        // Stop watchdog first
        let _ = self.watchdog.await;

        // Wait for all workers; detect panics
        for (i, handle) in self.workers.into_iter().enumerate() {
            match handle.await {
                Ok(()) => {}
                Err(e) if e.is_panic() => {
                    warn!(worker_id = i, "worker panicked: {:?}", e);
                }
                Err(e) => {
                    warn!(worker_id = i, "worker join error: {}", e);
                }
            }
        }

        info!("worker pool shut down");
    }

    /// Signal workers to stop accepting new tasks.
    /// Already in-progress downloads will complete.
    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}
