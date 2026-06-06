//! Integration test: fetch gallery details and get download URL.
//!
//! Requires network access. Uses proxy if available.
//! Run with: cargo test -p nh-tests --test download_test -- --nocapture

use std::io::Write;

use nh_api::endpoints::{DownloadFormat, GalleryEndpoints};
use nh_api::image_url::build_image_url;
use nh_api::NhClient;
use nh_tests::{test_client_config, TEST_GALLERY_ID, TEST_PROXY};

#[tokio::test]
async fn fetch_gallery_details() {
    let config = test_client_config();
    let client = NhClient::new(config)
        .await
        .expect("failed to create NhClient");

    let gallery = client
        .get_gallery(TEST_GALLERY_ID, None)
        .await
        .expect("failed to fetch gallery");

    assert_eq!(gallery.id, TEST_GALLERY_ID);
    assert!(!gallery.media_id.is_empty());
    assert!(gallery.num_pages > 0);
    assert!(gallery.num_favorites > 0);
    assert!(!gallery.tags.is_empty());
}

#[tokio::test]
async fn fetch_download_url() {
    let config = test_client_config();
    let client = NhClient::new(config)
        .await
        .expect("failed to create NhClient");

    let download = client
        .download_gallery(TEST_GALLERY_ID, DownloadFormat::Zip)
        .await
        .expect("failed to get download URL");

    assert!(download.url.starts_with("https://"));
    assert!(download.expires_at > 0);
}

#[tokio::test]
async fn download_zip_file() {
    let config = test_client_config();
    let client = NhClient::new(config)
        .await
        .expect("failed to create NhClient");

    let download = client
        .download_gallery(TEST_GALLERY_ID, DownloadFormat::Zip)
        .await
        .expect("failed to get download URL");

    let download_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(TEST_PROXY).unwrap())
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("failed to build download client");

    // Retry up to 3 times (CDN TLS can be flaky through proxy)
    let mut bytes = None;
    for attempt in 1..=3 {
        match download_client
            .get(&download.url)
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
            .header(reqwest::header::REFERER, "https://nhentai.net/")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                bytes = Some(resp.bytes().await.expect("failed to read bytes"));
                break;
            }
            Ok(resp) => {
                println!("Attempt {}: HTTP {}", attempt, resp.status());
            }
            Err(e) => {
                println!("Attempt {} failed: {}", attempt, e);
            }
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    let bytes = bytes.expect("all download attempts failed");
    assert!(
        bytes.len() > 1_000_000,
        "zip file too small: {} bytes",
        bytes.len()
    );

    // Save to disk
    let download_dir = "d:/code/nH-Viewer/downloads";
    std::fs::create_dir_all(download_dir).expect("failed to create dir");
    let path = format!("{}/{}.zip", download_dir, TEST_GALLERY_ID);
    let mut file = std::fs::File::create(&path).expect("failed to create file");
    file.write_all(&bytes).expect("failed to write file");
    println!("Saved {} bytes to {}", bytes.len(), path);
}

#[tokio::test]
async fn cdn_config_image_urls() {
    let config = test_client_config();
    let client = NhClient::new(config)
        .await
        .expect("failed to create NhClient");

    let cdn = client.cdn_config();
    assert!(!cdn.image_servers.is_empty());

    let gallery = client
        .get_gallery(TEST_GALLERY_ID, None)
        .await
        .expect("failed to fetch gallery");

    if !gallery.pages.is_empty() {
        let url = build_image_url(cdn, &gallery.pages[0].path, gallery.id as usize);
        assert!(url.starts_with("https://"));
        assert!(url.contains("/galleries/"));
    }
}
