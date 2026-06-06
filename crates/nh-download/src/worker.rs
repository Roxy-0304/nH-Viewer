use std::sync::Arc;

use reqwest::Client;
use tokio::sync::watch;
use tracing::{debug, error, info};

use nh_api::CdnConfig;

use crate::chunked::ChunkDownloader;
use crate::progress::DynReporter;
use crate::queue::DownloadQueue;

/// Worker pool that runs concurrent download workers.
///
/// Each worker maintains its own HTTP client for connection pooling efficiency.
pub struct WorkerPool {
    workers: Vec<tokio::task::JoinHandle<()>>,
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
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
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

                    let task = match task {
                        Some(t) => t,
                        None => {
                            debug!(worker_id, "no more tasks, worker idle");
                            // Wait for shutdown or new tasks
                            let _ = rx.changed().await;
                            if *rx.borrow() {
                                break;
                            }
                            continue;
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

                    // Build the download URL from CDN config + path stored in task
                    let url = format!("{}{}", cdn.image_server(task.server_index), task.path);

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

                    // Perform the download
                    let reporter_ref: &dyn crate::progress::ProgressReporter = &*reporter;
                    match downloader
                        .download(&url, &dest_path, reporter_ref, task.id)
                        .await
                    {
                        Ok(bytes) => {
                            debug!(worker_id, task_id = task.id, bytes, "download succeeded");
                            q.complete(task.id).await;
                        }
                        Err(e) => {
                            error!(
                                worker_id,
                                task_id = task.id,
                                error = %e,
                                "download failed"
                            );
                            q.fail(task.id, e.to_string()).await;
                            reporter.on_error(task.id, &e.to_string());
                        }
                    }
                }
                info!(worker_id, "worker stopped");
            });

            workers.push(handle);
        }

        info!(concurrency, "worker pool started");

        Self {
            workers,
            shutdown_tx,
        }
    }

    /// Signal all workers to shut down and wait for them to finish.
    pub async fn shutdown(self) {
        info!("shutting down worker pool");
        let _ = self.shutdown_tx.send(true);
        for handle in self.workers {
            let _ = handle.await;
        }
        info!("worker pool shut down");
    }

    /// Signal workers to stop accepting new tasks.
    /// Already in-progress downloads will complete.
    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}
