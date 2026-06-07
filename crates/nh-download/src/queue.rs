use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::error::Result;

/// Download task priority.
///
/// Higher priority tasks are dequeued first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// Background pre-cache (lowest)
    Low = 0,
    /// Batch favorites download
    Medium = 1,
    /// User-initiated download (highest)
    High = 2,
}

/// State of a download task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// Waiting in the queue
    Pending,
    /// Currently being downloaded
    Downloading,
    /// Paused by user
    Paused,
    /// Successfully completed
    Completed,
    /// Failed with an error message
    Failed(String),
    /// Cancelled by user
    Cancelled,
}

/// A single download task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    /// Unique task id
    pub id: u64,
    /// Gallery ID this page belongs to
    pub gallery_id: u64,
    /// Media ID for URL generation
    pub media_id: String,
    /// 1-based page number
    pub page: u32,
    /// File extension (jpg, png, gif)
    pub ext: String,
    /// Download priority
    pub priority: Priority,
    /// Current state
    pub state: TaskState,
    /// Total bytes to download (0 until known)
    pub total_bytes: u64,
    /// Bytes downloaded so far
    pub downloaded_bytes: u64,
    /// CDN server index for load balancing
    pub server_index: usize,
    /// CDN-relative path from the API (e.g. "/galleries/1234/1.jpg")
    pub path: String,
    /// When this task was leased to a worker (for stale-task detection).
    /// Not persisted — resets to `None` on queue restore.
    #[serde(skip)]
    #[serde(default)]
    pub leased_at: Option<Instant>,
}

impl DownloadTask {
    /// Returns true if this task is in a terminal state (completed, failed, cancelled).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled
        )
    }

    /// Returns true if this task can be picked up by a worker.
    pub fn is_runnable(&self) -> bool {
        self.state == TaskState::Pending
    }
}

/// Thread-safe priority download queue.
///
/// Supports pause/resume/cancel of individual tasks and queue persistence.
pub struct DownloadQueue {
    inner: Arc<Mutex<VecDeque<DownloadTask>>>,
    notify: Arc<Notify>,
    next_id: Arc<Mutex<u64>>,
}

impl Clone for DownloadQueue {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            notify: Arc::clone(&self.notify),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

impl DownloadQueue {
    /// Create a new empty download queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            notify: Arc::new(Notify::new()),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Add a task to the queue. Returns the assigned task id.
    ///
    /// If a task for the same `(gallery_id, page)` already exists in a
    /// non-terminal state, the duplicate is silently skipped and the
    /// existing task id is returned.
    pub async fn add_task(
        &self,
        gallery_id: u64,
        media_id: String,
        page: u32,
        ext: String,
        priority: Priority,
        server_index: usize,
        path: String,
    ) -> u64 {
        // ---- Deduplication ----
        {
            let queue = self.inner.lock().await;
            if let Some(existing) = queue
                .iter()
                .find(|t| t.gallery_id == gallery_id && t.page == page && !t.is_terminal())
            {
                debug!(
                    existing_id = existing.id,
                    gallery_id, page, "duplicate task skipped"
                );
                return existing.id;
            }
        }

        let id = {
            let mut id_guard = self.next_id.lock().await;
            let id = *id_guard;
            *id_guard += 1;
            id
        };

        let task = DownloadTask {
            id,
            gallery_id,
            media_id,
            page,
            ext,
            priority,
            state: TaskState::Pending,
            total_bytes: 0,
            downloaded_bytes: 0,
            server_index,
            path,
            leased_at: None,
        };

        self.insert_sorted(task).await;
        self.notify.notify_one();
        debug!(task_id = id, "task added to queue");
        id
    }

    /// Insert a task into the queue maintaining priority order (highest first).
    async fn insert_sorted(&self, task: DownloadTask) {
        let mut queue = self.inner.lock().await;
        let pos = queue
            .iter()
            .position(|t| t.priority < task.priority)
            .unwrap_or(queue.len());
        queue.insert(pos, task);
    }

    /// Get the next pending task (highest priority).
    /// If no runnable tasks exist, waits for new tasks via notification.
    /// Returns `None` only when a shutdown signal is received.
    pub async fn next(&self) -> Option<DownloadTask> {
        loop {
            {
                let mut queue = self.inner.lock().await;
                if let Some(pos) = queue.iter().position(|t| t.is_runnable()) {
                    let mut task = queue.remove(pos).unwrap();
                    task.state = TaskState::Downloading;
                    task.leased_at = Some(Instant::now());
                    queue.push_front(task.clone());
                    return Some(task);
                }
            }
            // No runnable tasks found — wait for notification that new tasks were added
            self.notify.notified().await;
        }
    }

    /// Pause a task by id. Returns true if found and paused.
    pub async fn pause(&self, task_id: u64) -> bool {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            if task.state == TaskState::Pending || task.state == TaskState::Downloading {
                task.state = TaskState::Paused;
                return true;
            }
        }
        false
    }

    /// Resume a paused task. Returns true if found and resumed.
    pub async fn resume(&self, task_id: u64) -> bool {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            if task.state == TaskState::Paused {
                task.state = TaskState::Pending;
                self.notify.notify_one();
                return true;
            }
        }
        false
    }

    /// Cancel a task by id. Returns true if found and cancelled.
    pub async fn cancel(&self, task_id: u64) -> bool {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            if !task.is_terminal() {
                task.state = TaskState::Cancelled;
                return true;
            }
        }
        false
    }

    /// Mark a task as completed.
    pub async fn complete(&self, task_id: u64) {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            task.state = TaskState::Completed;
        }
        self.notify.notify_one();
    }

    /// Mark a task as failed.
    pub async fn fail(&self, task_id: u64, reason: String) {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            task.state = TaskState::Failed(reason);
        }
        self.notify.notify_one();
    }

    /// Update progress for a task.
    pub async fn update_progress(&self, task_id: u64, downloaded: u64, total: u64) {
        let mut queue = self.inner.lock().await;
        if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
            task.downloaded_bytes = downloaded;
            task.total_bytes = total;
        }
    }

    /// Get a snapshot of all tasks.
    pub async fn snapshot(&self) -> Vec<DownloadTask> {
        let queue = self.inner.lock().await;
        queue.iter().cloned().collect()
    }

    /// Get the count of pending tasks.
    pub async fn pending_count(&self) -> usize {
        let queue = self.inner.lock().await;
        queue.iter().filter(|t| t.is_runnable()).count()
    }

    /// Get the count of active (downloading) tasks.
    pub async fn active_count(&self) -> usize {
        let queue = self.inner.lock().await;
        queue
            .iter()
            .filter(|t| t.state == TaskState::Downloading)
            .count()
    }

    /// Pause all pending tasks.
    pub async fn pause_all(&self) {
        let mut queue = self.inner.lock().await;
        for task in queue.iter_mut() {
            if task.state == TaskState::Pending || task.state == TaskState::Downloading {
                task.state = TaskState::Paused;
            }
        }
    }

    /// Resume all paused tasks.
    pub async fn resume_all(&self) {
        let mut queue = self.inner.lock().await;
        for task in queue.iter_mut() {
            if task.state == TaskState::Paused {
                task.state = TaskState::Pending;
            }
        }
        self.notify.notify_waiters();
    }

    /// Cancel all non-terminal tasks.
    pub async fn cancel_all(&self) {
        let mut queue = self.inner.lock().await;
        for task in queue.iter_mut() {
            if !task.is_terminal() {
                task.state = TaskState::Cancelled;
            }
        }
    }

    /// Remove all tasks from the queue (both active and terminal).
    pub async fn clear(&self) {
        let mut queue = self.inner.lock().await;
        queue.clear();
        info!("queue cleared");
    }

    /// Serialize the queue state to JSON for persistence.
    pub async fn to_json(&self) -> String {
        let queue = self.inner.lock().await;
        let tasks: Vec<&DownloadTask> = queue.iter().collect();
        serde_json::to_string(&tasks).unwrap_or_default()
    }

    /// Restore the queue from a JSON string.
    /// Tasks that were in `Downloading` state are reset to `Pending`
    /// (since no worker is actually processing them after a restart).
    pub async fn from_json(&self, json: &str) -> Result<()> {
        let mut tasks: Vec<DownloadTask> = serde_json::from_str(json)?;
        let max_id = tasks.iter().map(|t| t.id).max().unwrap_or(0);
        // Reset in-progress tasks — they can't survive a restart.
        for task in &mut tasks {
            if task.state == TaskState::Downloading {
                task.state = TaskState::Pending;
            }
            task.leased_at = None;
        }
        let mut queue = self.inner.lock().await;
        queue.clear();
        for task in tasks {
            queue.push_back(task);
        }
        let mut next_id = self.next_id.lock().await;
        *next_id = max_id + 1;
        self.notify.notify_one();
        info!(count = queue.len(), "queue restored from persistence");
        Ok(())
    }

    /// Reap stale leases: reset `Downloading` tasks whose lease has expired
    /// back to `Pending` so another worker can pick them up.
    ///
    /// Called periodically by the worker-pool watchdog.
    pub async fn reap_stale_leases(&self, lease_timeout: Duration) {
        let mut queue = self.inner.lock().await;
        let mut reaped = 0u32;
        for task in queue.iter_mut() {
            if task.state == TaskState::Downloading {
                if let Some(leased_at) = task.leased_at {
                    if leased_at.elapsed() > lease_timeout {
                        warn!(
                            task_id = task.id,
                            gallery_id = task.gallery_id,
                            page = task.page,
                            elapsed_secs = leased_at.elapsed().as_secs(),
                            "stale lease detected, resetting to Pending"
                        );
                        task.state = TaskState::Pending;
                        task.leased_at = None;
                        reaped += 1;
                    }
                }
            }
        }
        if reaped > 0 {
            drop(queue); // release lock before notifying
            self.notify.notify_waiters();
            info!(reaped, "stale leases reaped");
        }
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new()
    }
}
