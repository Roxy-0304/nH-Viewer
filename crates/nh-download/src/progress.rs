use std::sync::Arc;

use tokio::sync::mpsc;

/// Progress event reported by workers.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Bytes downloaded for a specific task
    Progress {
        task_id: u64,
        bytes_downloaded: u64,
        total_bytes: u64,
    },
    /// Task completed successfully
    Complete { task_id: u64 },
    /// Task failed with an error message
    Error { task_id: u64, message: String },
}

/// Trait for receiving download progress notifications.
///
/// Implement this trait to receive callbacks during download.
/// For FFI (e.g. nh-bridge), implement this trait and forward
/// calls through the bridge layer.
pub trait ProgressReporter: Send + Sync + 'static {
    /// Called periodically with download progress.
    fn on_progress(&self, task_id: u64, bytes_downloaded: u64, total_bytes: u64);

    /// Called when a download task completes successfully.
    fn on_complete(&self, task_id: u64);

    /// Called when a download task fails.
    fn on_error(&self, task_id: u64, message: &str);
}

/// A no-op reporter that discards all events.
#[derive(Debug, Clone, Copy)]
pub struct NoopReporter;

impl ProgressReporter for NoopReporter {
    fn on_progress(&self, _task_id: u64, _bytes_downloaded: u64, _total_bytes: u64) {}
    fn on_complete(&self, _task_id: u64) {}
    fn on_error(&self, _task_id: u64, _message: &str) {}
}

/// A logging reporter that emits progress events via `tracing`.
#[derive(Debug, Clone, Copy)]
pub struct LoggingReporter;

impl ProgressReporter for LoggingReporter {
    fn on_progress(&self, task_id: u64, bytes_downloaded: u64, total_bytes: u64) {
        let pct = if total_bytes > 0 {
            (bytes_downloaded * 100) / total_bytes
        } else {
            0
        };
        tracing::debug!(
            task_id,
            bytes_downloaded,
            total_bytes,
            pct,
            "download progress"
        );
    }

    fn on_complete(&self, task_id: u64) {
        tracing::info!(task_id, "download complete");
    }

    fn on_error(&self, task_id: u64, message: &str) {
        tracing::error!(task_id, message, "download failed");
    }
}

/// A channel-based reporter that sends events to a tokio mpsc receiver.
///
/// Useful for collecting events in a background task or for testing.
#[derive(Debug, Clone)]
pub struct ChannelReporter {
    tx: mpsc::UnboundedSender<ProgressEvent>,
}

impl ChannelReporter {
    /// Create a new channel-based reporter.
    /// Returns the reporter and the receiving end of the channel.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ProgressEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

impl ProgressReporter for ChannelReporter {
    fn on_progress(&self, task_id: u64, bytes_downloaded: u64, total_bytes: u64) {
        let _ = self.tx.send(ProgressEvent::Progress {
            task_id,
            bytes_downloaded,
            total_bytes,
        });
    }

    fn on_complete(&self, task_id: u64) {
        let _ = self.tx.send(ProgressEvent::Complete { task_id });
    }

    fn on_error(&self, task_id: u64, message: &str) {
        let _ = self.tx.send(ProgressEvent::Error {
            task_id,
            message: message.to_string(),
        });
    }
}

/// Type-erased progress reporter for dynamic dispatch.
pub type DynReporter = Arc<dyn ProgressReporter>;
