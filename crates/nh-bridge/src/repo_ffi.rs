//! FFI entry-points backed by `GalleryRepository` (mock or real).
//!
//! These functions work through the `GalleryRepository` trait, so the same
//! Dart code can work with mock data or real SQLite storage.

use std::ffi::c_int;
use std::sync::{Mutex, OnceLock};

use nh_storage::repository::GalleryRepository;

use crate::types::NhString;

// ---------------------------------------------------------------------------
// Global repository
// ---------------------------------------------------------------------------

/// The active gallery repository (mock or real).
/// Initialized by `nh_init_mock()` or `nh_init_real()`.
static REPOSITORY: OnceLock<Mutex<Box<dyn GalleryRepository>>> = OnceLock::new();

fn repo() -> &'static Mutex<Box<dyn GalleryRepository>> {
    REPOSITORY
        .get()
        .expect("Repository not initialized — call nh_init_mock() or nh_init_real() first")
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Initialize with mock (in-memory, hardcoded) data. No network or disk required.
/// Returns 0 on success.
///
/// Must be called **after** `nh_init`.
#[no_mangle]
pub extern "C" fn nh_init_mock() -> c_int {
    let mock_repo: Box<dyn GalleryRepository> =
        Box::new(nh_storage::mock::MockGalleryRepository::new());
    let _ = REPOSITORY.set(Mutex::new(mock_repo));
    0
}

/// Initialize with real (SQLite-backed) storage.
/// The `nh_init()` function must have been called first so that the tokio
/// runtime is available. This will create a `Storage` instance using
/// platform-default settings.
///
/// Returns 0 on success, negative on failure.
#[no_mangle]
pub extern "C" fn nh_init_real() -> c_int {
    let runtime = super::to_flutter::RUNTIME.get().expect("nh_init not called");

    let result = runtime.block_on(async {
        let settings = nh_storage::settings::Settings::default_for_platform()
            .map_err(|e| format!("settings: {e}"))?;
        let storage = nh_storage::Storage::init(settings)
            .await
            .map_err(|e| format!("storage: {e}"))?;
        Ok::<_, String>(storage)
    });

    match result {
        Ok(storage) => {
            let repo: Box<dyn GalleryRepository> = Box::new(storage);
            let _ = REPOSITORY.set(Mutex::new(repo));
            0
        }
        Err(e) => {
            tracing::error!("nh_init_real failed: {e}");
            -1
        }
    }
}

// ---------------------------------------------------------------------------
// Repository operations
// ---------------------------------------------------------------------------

/// Fetch a gallery by id. Returns JSON `CachedGallery` or `null`.
///
/// # Safety
/// Caller must free the returned string with `nh_string_free`.
#[no_mangle]
pub extern "C" fn nh_repo_gallery_get(gallery_id: u64) -> NhString {
    let repo = repo();
    let runtime = crate::to_flutter::RUNTIME
        .get()
        .expect("nh_init not called");

    match runtime.block_on(async {
        let r = repo.lock().unwrap();
        r.get_gallery(gallery_id).await
    }) {
        Ok(Some(gallery)) => match serde_json::to_string(&gallery) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Ok(None) => NhString::from_str("null"),
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

/// Search galleries by title. Returns JSON `Vec<GalleryPreview>`.
///
/// # Safety
/// `query` must be a valid UTF-8 C string.
#[no_mangle]
pub extern "C" fn nh_repo_search(query: *const std::ffi::c_char, page: u32) -> NhString {
    let q = if query.is_null() {
        return NhString::from_str(r#"{"error":"null query"}"#);
    } else {
        match unsafe { std::ffi::CStr::from_ptr(query) }.to_str() {
            Ok(s) => s,
            Err(e) => return NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
        }
    };

    let repo = repo();
    let runtime = crate::to_flutter::RUNTIME
        .get()
        .expect("nh_init not called");

    match runtime.block_on(async {
        let r = repo.lock().unwrap();
        r.search_galleries(q, page).await
    }) {
        Ok(results) => match serde_json::to_string(&results) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

/// List all galleries (page 1). Returns JSON `Vec<GalleryPreview>`.
/// This is a convenience wrapper that searches with an empty query.
///
/// # Safety
/// Caller must free the returned string with `nh_string_free`.
#[no_mangle]
pub extern "C" fn nh_repo_list(page: u32) -> NhString {
    let repo = repo();
    let runtime = crate::to_flutter::RUNTIME
        .get()
        .expect("nh_init not called");

    match runtime.block_on(async {
        let r = repo.lock().unwrap();
        // Empty string matches everything in mock search
        r.search_galleries("", page).await
    }) {
        Ok(results) => match serde_json::to_string(&results) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}

/// List search history. Returns JSON `Vec<SearchHistoryItem>`.
///
/// # Safety
/// Caller must free the returned string with `nh_string_free`.
#[no_mangle]
pub extern "C" fn nh_repo_search_history(limit: u32) -> NhString {
    let repo = repo();
    let runtime = crate::to_flutter::RUNTIME
        .get()
        .expect("nh_init not called");

    match runtime.block_on(async {
        let r = repo.lock().unwrap();
        r.list_search_history(limit).await
    }) {
        Ok(results) => match serde_json::to_string(&results) {
            Ok(json) => NhString::from_str(&json),
            Err(e) => NhString::from_str(&format!("{{\"error\":\"serialize: {e}\"}}")),
        },
        Err(e) => NhString::from_str(&format!("{{\"error\":\"{e}\"}}")),
    }
}