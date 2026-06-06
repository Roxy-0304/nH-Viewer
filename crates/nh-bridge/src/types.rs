use std::ffi::c_char;

/// `#[repr(C)]` string handle for Dart FFI.
///
/// Dart side reads `ptr` and `len` directly from memory.
/// Must be freed via `nh_string_free`.
#[repr(C)]
pub struct NhString {
    pub ptr: *mut c_char,
    pub len: u32,
}

impl NhString {
    /// Create from a Rust `&str`.
    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let len = bytes.len();
        // Allocate a null-terminated buffer
        let buf = std::ffi::CString::new(s.replace('\0', ""))
            .unwrap_or_default();
        let ptr = buf.into_raw();
        Self {
            ptr,
            len: len as u32,
        }
    }
}

/// Opaque download task handle passed to Dart as a pointer.
///
/// Dart holds a `Pointer<NhDownloadTask>` (the raw pointer value).
/// `nh_download_cancel` consumes it via `Box::from_raw`.
#[repr(C)]
pub struct NhDownloadTask {
    pub gallery_id: u32,
}

impl NhDownloadTask {
    pub fn new(gallery_id: u32) -> Self {
        Self { gallery_id }
    }
}

/// Download status codes.
#[repr(i32)]
pub enum DownloadStatus {
    Pending = 0,
    Running = 1,
    Done = 2,
    Failed = -1,
}