use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::progress::ProgressReporter;

/// Default chunk size: 512 KB
const DEFAULT_CHUNK_SIZE: u64 = 512 * 1024;

/// Maximum retry attempts per chunk
const MAX_RETRIES: u32 = 5;

/// Base delay for exponential backoff
const BASE_DELAY: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Top-level convenience function (requested API)
// ---------------------------------------------------------------------------

/// Download a single file with chunked HTTP Range requests and resume support.
///
/// # Arguments
/// * `client` — reqwest HTTP client to use
/// * `url` — the URL to download from
/// * `save_path` — the final destination path (a `.tmp` sibling is used during download)
/// * `task_id` — an opaque task identifier passed to `reporter`
/// * `reporter` — progress callback sink
///
/// # Behaviour
/// 1. If a `.tmp` file already exists at `save_path.tmp`, its length is used as the
///    starting byte offset (resume).
/// 2. A `HEAD` request probes the total file size (via `Content-Length`).
/// 3. Data is streamed in chunks of [`DEFAULT_CHUNK_SIZE`] using `Range` headers.
/// 4. Each chunk is written immediately to disk via [`tokio::io::AsyncWriteExt`],
///    keeping memory usage bounded.
/// 5. After every chunk the reporter is notified via
///    `reporter.update_progress(task_id, downloaded, total)`.
/// 6. On completion the `.tmp` file is atomically renamed to `save_path`.
///
/// # Error handling
/// * Network timeouts are propagated from the underlying reqwest client.
/// * HTTP 416 (Range Not Satisfiable) is treated as "already complete".
/// * Each chunk is retried up to [`MAX_RETRIES`] times with exponential backoff.
pub async fn download_file_chunked(
    client: &Client,
    url: &str,
    save_path: &Path,
    task_id: u64,
    reporter: Arc<dyn ProgressReporter>,
) -> anyhow::Result<()> {
    let downloader = ChunkDownloader::new(client.clone());
    let reporter_ref: &dyn ProgressReporter = &*reporter;
    downloader
        .download(url, save_path, reporter_ref, task_id)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ChunkDownloader struct (builder pattern, reusable)
// ---------------------------------------------------------------------------

/// Chunk downloader with HTTP Range request support and retry logic.
#[derive(Debug)]
pub struct ChunkDownloader {
    client: Client,
    chunk_size: u64,
    max_retries: u32,
}

impl ChunkDownloader {
    /// Create a new chunk downloader with the given HTTP client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            chunk_size: DEFAULT_CHUNK_SIZE,
            max_retries: MAX_RETRIES,
        }
    }

    /// Set the chunk size for Range requests.
    pub fn with_chunk_size(mut self, size: u64) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set the maximum retry attempts per chunk.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Download a file from `url` to `dest_path` using HTTP Range requests.
    ///
    /// - If `dest_path` already exists and has partial content, resumes from that point.
    /// - Writes to a `.tmp` file, then atomically renames on completion.
    /// - Reports progress via `reporter`.
    ///
    /// Returns the total bytes downloaded.
    pub async fn download(
        &self,
        url: &str,
        dest_path: &Path,
        reporter: &dyn ProgressReporter,
        task_id: u64,
    ) -> Result<u64> {
        let tmp_path = dest_path.with_extension("tmp");

        // Probe total size with a HEAD request first
        let total_bytes = self.probe_size(url).await?;

        // ---- Resume validation (方案 B) ----
        let existing_bytes = if tmp_path.exists() {
            let meta = fs::metadata(&tmp_path).await.map(|m| m.len()).unwrap_or(0);
            meta
        } else {
            0
        };

        debug!(url, total_bytes, existing_bytes, "download started");

        // Case: total_bytes == 0 (HEAD failed or server doesn't report size)
        if total_bytes == 0 && existing_bytes > 0 {
            warn!(
                tmp_path = %tmp_path.display(),
                "cannot validate .tmp (total_bytes unknown), deleting and re-downloading"
            );
            let _ = fs::remove_file(&tmp_path).await;
            // fall through to fresh download with existing_bytes = 0
        }
        // Case: .tmp is larger than server reports → stale or corrupt
        else if total_bytes > 0 && existing_bytes > total_bytes {
            warn!(
                existing_bytes,
                total_bytes, ".tmp is larger than expected, deleting and re-downloading"
            );
            let _ = fs::remove_file(&tmp_path).await;
            // fall through
        }
        // Case: sizes match → likely complete
        else if total_bytes > 0 && existing_bytes == total_bytes {
            // Additional check: if the .tmp file is older than 7 days, treat it
            // as stale (the remote file may have changed).
            let stale = fs::metadata(&tmp_path)
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .map(|d| d > Duration::from_secs(7 * 24 * 3600))
                .unwrap_or(true); // if we can't read mtime, treat as stale

            if stale {
                warn!(
                    tmp_path = %tmp_path.display(),
                    ".tmp is old (>7d) or mtime unavailable, deleting and re-downloading"
                );
                let _ = fs::remove_file(&tmp_path).await;
            } else {
                // Looks good — rename to final
                debug!("complete .tmp found, renaming to final");
                fs::rename(&tmp_path, dest_path).await?;
                reporter.on_complete(task_id);
                return Ok(total_bytes);
            }
        }
        // Case: existing_bytes < total_bytes → normal resume (fall through)

        // Re-read existing_bytes after possible deletion above
        let existing_bytes = if tmp_path.exists() {
            fs::metadata(&tmp_path).await.map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        // Guard: if we still don't know the total size and there's nothing to
        // resume, bail out rather than creating an empty file.
        if total_bytes == 0 {
            return Err(Error::ChunkFailed {
                url: url.to_string(),
                retries: 0,
                reason: "server did not report Content-Length; cannot download".to_string(),
            });
        }

        // Open file in append mode (resume) or create new
        let mut file = if existing_bytes > 0 {
            debug!(existing_bytes, "resuming download");
            OpenOptions::new().append(true).open(&tmp_path).await?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = tmp_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            File::create(&tmp_path).await?
        };

        let mut downloaded = existing_bytes;

        // Download in chunks using Range requests
        while downloaded < total_bytes {
            let range_start = downloaded;
            let range_end = std::cmp::min(downloaded + self.chunk_size - 1, total_bytes - 1);

            let chunk_data = self
                .download_chunk(url, range_start, range_end)
                .await
                .map_err(|e| Error::ChunkFailed {
                    url: url.to_string(),
                    retries: self.max_retries,
                    reason: e.to_string(),
                })?;

            file.write_all(&chunk_data).await?;
            downloaded += chunk_data.len() as u64;

            reporter.on_progress(task_id, downloaded, total_bytes);
        }

        file.flush().await?;
        drop(file);

        // Atomic rename: tmp → final
        fs::rename(&tmp_path, dest_path).await?;

        debug!(total_bytes = downloaded, "download complete");
        reporter.on_complete(task_id);
        Ok(downloaded)
    }

    /// Probe the total file size with a HEAD request.
    async fn probe_size(&self, url: &str) -> Result<u64> {
        let resp = self.client.head(url).send().await?;
        let content_length = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(content_length)
    }

    /// Download a single chunk [range_start, range_end] with exponential backoff retries.
    async fn download_chunk(&self, url: &str, range_start: u64, range_end: u64) -> Result<Vec<u8>> {
        let range_header = format!("bytes={}-{}", range_start, range_end);

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = BASE_DELAY * 2u32.pow(attempt - 1);
                warn!(
                    attempt,
                    url,
                    range_start,
                    range_end,
                    ?delay,
                    "retrying chunk download"
                );
                tokio::time::sleep(delay).await;
            }

            match self
                .client
                .get(url)
                .header(reqwest::header::RANGE, &range_header)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();

                    if status == reqwest::StatusCode::OK
                        || status == reqwest::StatusCode::PARTIAL_CONTENT
                    {
                        let data = resp.bytes().await?;
                        return Ok(data.to_vec());
                    }

                    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                        // Range not satisfiable — file may be smaller than expected
                        // Return empty to indicate we're past the end
                        return Ok(Vec::new());
                    }

                    warn!(status = %status, "unexpected status code in chunk download");
                }
                Err(e) => {
                    warn!(error = %e, "chunk download request failed");
                }
            }
        }

        Err(Error::ChunkFailed {
            url: url.to_string(),
            retries: self.max_retries,
            reason: "all retries exhausted".to_string(),
        })
    }
}
