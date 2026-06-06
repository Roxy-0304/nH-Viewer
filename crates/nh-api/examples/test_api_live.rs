//! 真实 API 测试 —— 使用代理、Cookie、完整浏览器指纹
//! 策略：先访问主页获取 cf_clearance cookie，再调用 API
use nh_api::{NhClient, GalleryEndpoints};
use std::time::Duration;

const API_KEY: &str = "nhk_3ctklttHlQf3la3PlyYH_Z95aWzt1xHZvFDrye_PYJ1kCW8q";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable tracing
    let _ = tracing_subscriber::fmt::try_init();

    println!("=== Live API Test ===\n");

    // 1. Build client via ClientConfig
    let config = nh_api::ClientConfig::new(API_KEY)
        .proxy("http://127.0.0.1:7897")
        .cookies(true)
        .max_retries(3)
        .timeout(Duration::from_secs(60));

    let client = NhClient::new(config).await?;
    println!("[PASS] Client initialized (proxy + cookies + browser headers)\n");

    // 2. CDN config (already works)
    let cdn = client.cdn_config();
    println!(
        "[PASS] CDN config: {} image servers, {} thumb servers",
        cdn.image_servers.len(),
        cdn.thumb_servers.len()
    );
    for (i, s) in cdn.image_servers.iter().enumerate() {
        println!("  image[{}]: {}", i, s);
    }
    println!();

    // 3. Try to warm up by visiting the main page first (get cf_clearance cookie)
    println!("--- Warming up: visiting nhentai.net main page ---");
    match client.warm_up().await {
        Ok(status) => println!("[INFO] Warm-up response: HTTP {}", status),
        Err(e) => println!("[WARN] Warm-up failed: {:#?}", e),
    }
    println!();

    // 4. Get gallery
    match client.get_gallery(177013).await {
        Ok(gallery) => {
            println!("[PASS] get_gallery(177013):");
            println!("  id:          {}", gallery.id);
            println!("  media_id:    {}", gallery.media_id);
            println!("  title:       {}", gallery.best_title());
            println!("  pages:       {}", gallery.num_pages);
            println!("  tags:        {}", gallery.tags.len());
            println!();
        }
        Err(e) => {
            println!("[FAIL] get_gallery(177013): {:#?}", e);
            println!();
        }
    }

    // 5. Search
    match client.search("full color", 1).await {
        Ok(results) => {
            println!("[PASS] search(\"full color\", 1):");
            println!("  results:     {}", results.result.len());
            println!("  total_pages: {}", results.num_pages);
            if let Some(first) = results.result.first() {
                println!("  first:       {} ({})", first.best_title(), first.id);
            }
            println!();
        }
        Err(e) => {
            println!("[FAIL] search: {:#?}", e);
            println!();
        }
    }

    // 6. Tagged
    match client.tagged(1, 1).await {
        Ok(results) => {
            println!("[PASS] tagged(tag_id=1, page=1):");
            println!("  results:     {}", results.result.len());
            println!();
        }
        Err(e) => {
            println!("[FAIL] tagged: {:#?}", e);
            println!();
        }
    }

    // 7. All
    match client.all(1).await {
        Ok(results) => {
            println!("[PASS] all(page=1):");
            println!("  results:     {}", results.result.len());
            println!();
        }
        Err(e) => {
            println!("[FAIL] all: {:#?}", e);
            println!();
        }
    }

    println!("=== Done ===");
    Ok(())
}