//! Bridge integration test: verify the bridge layer works end-to-end.
//!
//! Requires network access and proxy.
//! Run with: cargo test -p nh-tests --test bridge_integration -- --nocapture

use nh_bridge::api::gallery::{self, GalleryInfo, GalleryPreviewInfo};
use nh_bridge::Storage;
use nh_storage::settings::Settings;
use nh_tests::{test_client_config, TEST_GALLERY_ID};

/// Build a test Settings pointing to a temp directory.
fn test_settings() -> Settings {
    Settings::from_data_dir("d:/code/nH-Viewer/target/test_data/bridge_test".into())
}

/// Validate that GalleryInfo fields are sane.
fn assert_gallery_info_valid(g: &GalleryInfo) {
    assert!(g.id > 0, "id should be > 0");
    assert!(!g.media_id.is_empty(), "media_id should not be empty");
    assert!(
        g.title_en.is_some() || g.title_jp.is_some() || g.title_pretty.is_some(),
        "at least one title should be present"
    );
    assert!(g.num_pages > 0, "num_pages should be > 0");
}

/// Validate that GalleryPreviewInfo fields are sane.
fn assert_preview_valid(p: &GalleryPreviewInfo) {
    assert!(p.id > 0, "id should be > 0");
    assert!(!p.title.is_empty(), "title should not be empty");
    assert!(p.num_pages > 0, "num_pages should be > 0");
}

#[tokio::test]
async fn bridge_init_and_fetch_gallery() {
    let settings = test_settings();
    settings.ensure_dirs().await.expect("failed to create dirs");

    let storage = Storage::init(settings).await.expect("failed to init storage");

    let config = test_client_config();
    gallery::init_bridge_with_config(storage, config).await;

    // Warm up
    let status = gallery::nh_warm_up().await.expect("warm up failed");
    assert!((200..400).contains(&status), "warm up status: {}", status);

    // Fetch gallery
    let info = gallery::nh_get_gallery(TEST_GALLERY_ID)
        .await
        .expect("failed to get gallery");
    assert_gallery_info_valid(&info);
    assert_eq!(info.id, TEST_GALLERY_ID);
    println!("Gallery: {} ({})", info.title_en.unwrap_or_default(), info.id);
}

#[tokio::test]
async fn bridge_search() {
    let settings = test_settings();
    settings.ensure_dirs().await.expect("failed to create dirs");

    let storage = Storage::init(settings).await.expect("failed to init storage");

    let config = test_client_config();
    gallery::init_bridge_with_config(storage, config).await;

    let results = gallery::nh_search_galleries("english".to_string(), 1)
        .await
        .expect("search failed");
    assert!(!results.is_empty(), "search should return results");
    for p in &results {
        assert_preview_valid(p);
    }
}

#[tokio::test]
async fn bridge_popular() {
    let settings = test_settings();
    settings.ensure_dirs().await.expect("failed to create dirs");

    let storage = Storage::init(settings).await.expect("failed to init storage");

    let config = test_client_config();
    gallery::init_bridge_with_config(storage, config).await;

    let results = gallery::nh_get_popular()
        .await
        .expect("popular failed");
    assert!(!results.is_empty(), "popular should return results");
}

#[tokio::test]
async fn bridge_random() {
    let settings = test_settings();
    settings.ensure_dirs().await.expect("failed to create dirs");

    let storage = Storage::init(settings).await.expect("failed to init storage");

    let config = test_client_config();
    gallery::init_bridge_with_config(storage, config).await;

    let id = gallery::nh_get_random()
        .await
        .expect("random failed");
    assert!(id > 0, "random gallery id should be > 0");
}