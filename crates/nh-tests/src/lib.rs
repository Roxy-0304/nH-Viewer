//! Centralized test crate for the nH-Viewer project.
//!
//! Run all tests: `cargo test -p nh-tests`
//! Run with output: `cargo test -p nh-tests -- --nocapture`

use nh_api::client::ClientConfig;
use nh_api::ProxyMode;

/// Default proxy address used by all tests that require network access.
pub const TEST_PROXY: &str = "http://127.0.0.1:7897";

/// Default gallery ID used by all tests that need to fetch a gallery.
pub const TEST_GALLERY_ID: u64 = 463142;

/// Create a `ClientConfig` with the default test proxy.
pub fn test_client_config() -> ClientConfig {
    ClientConfig::default().proxy_mode(ProxyMode::Custom(TEST_PROXY.to_string()))
}
