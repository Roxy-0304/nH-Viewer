//! C ABI entry-points exposed to Dart via `dart:ffi`.
//!
//! Every public `#[no_mangle]` function is callable from Dart.
//! All async work is dispatched onto a shared `tokio` runtime.

use std::ffi::{c_char, c_int, CStr};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use tracing::error;

use nh_api::{GalleryEndpoints, NhClient};
use nh_download::progress::ProgressReporter;
use nh_download::queue::Priority;
use nh_download::DownloadManager;
use nh_storage::settings::Settings;
use nh_storage::Storage;

use crate::types::{DownloadStatus, NhString};

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Shared tokio runtime created once during `nh_init`.
pub(crate) static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// nh-api client (created in nh_init).
static API_CLIENT: OnceLock<NhClient> = OnceLock::new();

/// Download manager wrapped in a Mutex because it needs mutable access.
static DOWNLOAD_MANAGER: OnceLock<Mutex<DownloadManager>> = OnceLock::new();

/// FFI progress callback – set by Dart via `nh_download_register_callback`.
static PROGRESS_CALLBACK: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());

// ---------------------------------------------------------------------------
// Progress callback bridge
// ---------------------------------------------------------------------------

/// Type signature for the Dart callback.
/// `task_id`, `bytes_downloaded`, `total_bytes`.
type ProgressCallback = extern "C" fn(u64, u64, u64);

/// A reporter that forwards calls through the FFI callback.
struct FfiReporter;

impl ProgressReporter for FfiReporter {
    fn on_progress(&self, task_id: u64, bytes_downloaded: u64, total_bytes: u64) {
        let ptr = PROGRESS_CALLBACK.load(Ordering::Acquire);
        if !ptr.is_null() {
            let cb: ProgressCallback = unsafe { std::mem::transmute(ptr) };
            cb(task_id, bytes_downloaded, total_bytes);
        }
    }

    fn on_complete(&self, _task_id: u64) {
        // Could add a separate callback later; for now use progress with 100 %
    }

    fn on_error(&self, _task_id: u64, _message: &str) {
        // Could add a separate callback later
    }
}

// ---------------------------------------------------------------------------
// Helper: get or panic
// ---------------------------------------------------------------------------

fn rt() -> &'static Runtime {
    RUNTIME
        .get()
        .expect("nh_init has not been called yet")
}

fn api_client() -> &'static NhClient {
    API_CLIENT
        .get()
        .expect("nh_init has not been called yet")
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Initialise the shared async runtime and nh-api client.
///
/// Must be called **once** before any other `nh_*` function.
///
/// # Safety
/// - `api_key` may be null (uses default) or a valid UTF-8 C string.
/// - `data_dir` may be null (uses platform default) or a valid UTF-8 C string.
/// - `max_thumbnail_cache_mb` controls the LRU thumbnail cache size.
/// - `concurrency` controls download worker count.
/// - Returns 0 on success, negative on failure.
#[no_mangle]
pub extern "C" fn nh_init(
    api_key: *const c_char,
    data_dir: *const c_char,
    max_thumbnail_cache_mb: c_int,
    concurrency: c_int,
) -> c_int {
    // Initialise tracing (ignore if already set)
    let _ = tracing_subscriber::fmt::try_init();

    // Parse optional api_key
    let key_str: Option<String> = if api_key.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(api_key) }
            .to_str()
            .ok()
            .map(|s| s.to_owned())
    };

    // Parse optional data_dir
    let dir_str: Option<String> = if data_dir.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(data_dir) }
            .to_str()
            .ok()
            .map(|s| s.to_owned())
    };

    // Create tokio runtime
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Failed to create tokio runtime: {e}");
            return -1;
        }
    };

    // Block on initialisation inside the runtime (borrow only)
    let result = runtime.block_on(async {
        // Create API client
        let client = NhClient::with_api_key(key_str).await?;

        // Create storage settings
        let mut settings = Settings::default_for_platform().map_err(|e| {
            nh_api::Error::CdnConfigFetch { reason: format!("settings: {e}") }
        })?;
        if let Some(dir) = dir_str {
            settings.data_dir = std::path::PathBuf::from(dir);
        }
        if max_thumbnail_cache_mb > 0 {
            settings.max_thumbnail_cache_size =
                (max_thumbnail_cache_mb as u64) * 1024 * 1024;
        }

        let storage = Storage::init(settings).await.map_err(|e| {
            nh_api::Error::CdnConfigFetch { reason: format!("storage init: {e}") }
        })?;

        let conc = if concurrency > 0 {
            concurrency as usize
        } else {
            4
        };

        let manager =
            DownloadManager::new(client.clone(), storage, conc, FfiReporter)
                .await
                .map_err(|e| nh_api::Error::CdnConfigFetch { reason: format!("download manager init: {e}") })?;

        Ok::<(NhClient, Mutex<DownloadManager>), nh_api::Error>((client, Mutex::new(manager)))
    });

    match result {
        Ok((client, manager_mutex)) => {
            // Store globals after async work is done
            let _ = RUNTIME.set(runtime);
            let _ = API_CLIENT.set(client);
            let _ = DOWNLOAD_MANAGER.set(manager_mutex);
            0
        }
        Err(e) => {
            error!("nh_init failed: {e}");
            -2
        }
    }
}

// ---------------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------------

/// Fetch gallery metadata. Returns a JSON `NhString`.
///
/// The caller must free the returned string with `nh_string_free`.
///
/// # Safety
/// - `gallery_id` must be a positive integer.
#[no_mangle]
pub extern "C" fn nh_gallery_get(gallery_id: u64) -> NhString {
    let client = api_client();
    match rt().block_on(client.get_gallery(gallery_id)) {
        Ok(gallery) => match serde_json::to_string(&gallery) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

// ---------------------------------------------------------------------------
// Search / Tagged / All
// ---------------------------------------------------------------------------

/// Search nhentai. Returns JSON `NhString`.
///
/// # Safety
/// - `query` must be a valid UTF-8 C string.
/// - `page` is 1-based.
#[no_mangle]
pub extern "C" fn nh_search(query: *const c_char, page: u32) -> NhString {
    let q = if query.is_null() {
        return NhString::from_str(r#"{"error":"null query"}"#);
    } else {
        match unsafe { CStr::from_ptr(query) }.to_str() {
            Ok(s) => s,
            Err(e) => return NhString::from_str(&format!(r#"{{"error":"{e}"}}"#)),
        }
    };

    let client = api_client();
    match rt().block_on(client.search(q, page)) {
        Ok(resp) => match serde_json::to_string(&resp) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

/// Fetch galleries with a specific tag. Returns JSON `NhString`.
///
/// # Safety
/// - `tag` must be a valid UTF-8 C string.
#[no_mangle]
pub extern "C" fn nh_tagged(tag: *const c_char, page: u32) -> NhString {
    let t = if tag.is_null() {
        return NhString::from_str(r#"{"error":"null tag"}"#);
    } else {
        match unsafe { CStr::from_ptr(tag) }.to_str() {
            Ok(s) => s,
            Err(e) => return NhString::from_str(&format!(r#"{{"error":"{e}"}}"#)),
        }
    };

    let tag_id: u64 = match t.parse() {
        Ok(id) => id,
        Err(e) => return NhString::from_str(&format!(r#"{{"error":"invalid tag_id: {e}"}}"#)),
    };

    let client = api_client();
    match rt().block_on(client.tagged(tag_id, page)) {
        Ok(resp) => match serde_json::to_string(&resp) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

/// Fetch all galleries. Returns JSON `NhString`.
///
/// # Safety
/// - `page` is 1-based.
#[no_mangle]
pub extern "C" fn nh_all(page: u32) -> NhString {
    let client = api_client();
    match rt().block_on(client.all(page)) {
        Ok(resp) => match serde_json::to_string(&resp) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

// ---------------------------------------------------------------------------
// CDN
// ---------------------------------------------------------------------------

/// Return the current CDN config as a JSON `NhString`.
#[no_mangle]
pub extern "C" fn nh_cdn_config() -> NhString {
    let client = api_client();
    let cdn = client.cdn_config();
    match serde_json::to_string(cdn) {
        Ok(json) => NhString::from_str(&json),
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

// ---------------------------------------------------------------------------
// Image URL helpers
// ---------------------------------------------------------------------------

/// Build an image URL for a gallery page.
///
/// # Safety
/// - `media_id` must be a valid UTF-8 C string.
/// - `ext` must be a valid UTF-8 C string (e.g. "jpg", "png").
#[no_mangle]
pub extern "C" fn nh_image_url(
    media_id: *const c_char,
    page: u32,
    ext: *const c_char,
    is_thumbnail: bool,
) -> NhString {
    let mid = if media_id.is_null() {
        return NhString::from_str("");
    } else {
        match unsafe { CStr::from_ptr(media_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return NhString::from_str(""),
        }
    };

    let ext_str = if ext.is_null() {
        "jpg"
    } else {
        match unsafe { CStr::from_ptr(ext) }.to_str() {
            Ok(s) => s,
            Err(_) => "jpg",
        }
    };

    let client = api_client();
    let cdn = client.cdn_config();
    let url = if is_thumbnail {
        nh_api::image_url::get_thumbnail_url(cdn, mid, page, 0)
    } else {
        nh_api::image_url::get_image_url(cdn, mid, page, ext_str, 0)
    };
    NhString::from_str(&url)
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

/// Start downloading a gallery. Returns 0 on success.
///
/// # Safety
/// - `gallery_id` must be a positive integer.
#[no_mangle]
pub extern "C" fn nh_download_start(gallery_id: u64) -> c_int {
    let manager_guard = match DOWNLOAD_MANAGER.get() {
        Some(m) => m,
        None => return -1,
    };

    match rt().block_on(async {
        let mut manager = manager_guard.lock().await;
        manager
            .download_gallery(gallery_id, Priority::High)
            .await
    }) {
        Ok(()) => 0,
        Err(e) => {
            error!("nh_download_start failed: {e}");
            -2
        }
    }
}

/// Get the download status for a gallery.
///
/// Returns a status code (see `DownloadStatus`).
/// If `out_progress` is non-null, writes (downloaded_bytes, total_bytes) as two u64 values.
#[no_mangle]
pub extern "C" fn nh_download_status(
    gallery_id: u64,
    out_downloaded: *mut u64,
    out_total: *mut u64,
) -> DownloadStatus {
    let manager_guard = match DOWNLOAD_MANAGER.get() {
        Some(m) => m,
        None => return DownloadStatus::Failed,
    };

    match rt().block_on(async {
        let manager = manager_guard.lock().await;
        let snapshot = manager.task_snapshot().await;
        let tasks: Vec<_> = snapshot
            .into_iter()
            .filter(|t| t.gallery_id == gallery_id)
            .collect();

        if tasks.is_empty() {
            return Ok::<(DownloadStatus, u64, u64), nh_download::error::Error>((
                DownloadStatus::Pending,
                0,
                0,
            ));
        }

        let total_downloaded: u64 = tasks.iter().map(|t| t.downloaded_bytes).sum();
        let total_bytes: u64 = tasks.iter().map(|t| t.total_bytes).sum();
        let all_done = tasks.iter().all(|t| {
            matches!(
                t.state,
                nh_download::queue::TaskState::Completed
            )
        });
        let any_failed = tasks.iter().any(|t| {
            matches!(
                t.state,
                nh_download::queue::TaskState::Failed(_)
            )
        });

        let status = if all_done {
            DownloadStatus::Done
        } else if any_failed {
            DownloadStatus::Failed
        } else {
            DownloadStatus::Running
        };

        Ok((status, total_downloaded, total_bytes))
    }) {
        Ok((status, downloaded, total)) => {
            if !out_downloaded.is_null() {
                unsafe { *out_downloaded = downloaded; }
            }
            if !out_total.is_null() {
                unsafe { *out_total = total; }
            }
            status
        }
        Err(_) => DownloadStatus::Failed,
    }
}

/// Cancel a download for a gallery. Returns 0 on success.
#[no_mangle]
pub extern "C" fn nh_download_cancel(gallery_id: u64) -> c_int {
    let manager_guard = match DOWNLOAD_MANAGER.get() {
        Some(m) => m,
        None => return -1,
    };

    match rt().block_on(async {
        let manager = manager_guard.lock().await;
        let snapshot = manager.task_snapshot().await;
        let mut cancelled = 0u32;
        for task in snapshot.iter().filter(|t| t.gallery_id == gallery_id) {
            if manager.cancel(task.id).await {
                cancelled += 1;
            }
        }
        Ok::<u32, nh_download::error::Error>(cancelled)
    }) {
        Ok(_) => 0,
        Err(e) => {
            error!("nh_download_cancel failed: {e}");
            -2
        }
    }
}

/// Register a progress callback. The callback will be invoked from a
/// background tokio task.
///
/// # Safety
/// - `callback` must be a valid function pointer or null to unregister.
#[no_mangle]
pub extern "C" fn nh_download_register_callback(callback: *mut ()) {
    PROGRESS_CALLBACK.store(callback, Ordering::Release);
}

// ---------------------------------------------------------------------------
// Memory management
// ---------------------------------------------------------------------------

/// Free an `NhString` returned by this library.
///
/// # Safety
/// - `s` must have been returned by an `nh_*` function.
/// - After this call the `NhString` is invalid and must not be used.
#[no_mangle]
pub extern "C" fn nh_string_free(s: NhString) {
    if !s.ptr.is_null() {
        // Reconstruct the CString and drop it
        unsafe {
            let _ = std::ffi::CString::from_raw(s.ptr);
        }
    }
}