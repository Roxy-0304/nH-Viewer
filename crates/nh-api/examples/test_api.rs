//! 本地 Mock 测试 —— 无需联网，用于验证模型和接口
use nh_api::error::Result;
use nh_api::image_url;
use nh_api::types::*;
use nh_api::GalleryEndpoints;
use nh_api::CdnConfig;

struct MockClient;

impl GalleryEndpoints for MockClient {
    async fn get_gallery(&self, id: u64) -> Result<Gallery> {
        Ok(make_gallery(id))
    }
    async fn search(&self, _query: &str, page: u32) -> Result<PaginatedResponse<Gallery>> {
        Ok(make_paginated(page, 3))
    }
    async fn tagged(&self, _tag_id: u64, page: u32) -> Result<PaginatedResponse<Gallery>> {
        Ok(make_paginated(page, 5))
    }
    async fn all(&self, page: u32) -> Result<PaginatedResponse<Gallery>> {
        Ok(make_paginated(page, 10))
    }
}

fn make_gallery(id: u64) -> Gallery {
    Gallery {
        id,
        media_id: "123456789".to_string(),
        title: Title {
            english: Some("Test Gallery Title".to_string()),
            japanese: Some("テストギャラリータイトル".to_string()),
            pretty: Some("Test Gallery".to_string()),
        },
        images: Images {
            pages: vec![ImageFileType::Jpg, ImageFileType::Png, ImageFileType::Jpg],
            cover: ImageFileType::Jpg,
            thumbnail: ImageFileType::Jpg,
        },
        tags: vec![
            Tag { id: 1, name: "english".into(), tag_type: TagType::Language, url: "/tag/language:english/".into(), count: 1000000 },
            Tag { id: 2, name: "full color".into(), tag_type: TagType::Tag, url: "/tag/full-color/".into(), count: 500000 },
            Tag { id: 3, name: "artist_name".into(), tag_type: TagType::Artist, url: "/artist/artist_name/".into(), count: 50 },
        ],
        num_pages: 3,
        num_favorites: 42,
        scanlator: Some("".into()),
        upload_date: 1700000000,
    }
}

fn make_paginated(page: u32, total_pages: u32) -> PaginatedResponse<Gallery> {
    PaginatedResponse {
        result: (0..3).map(|i| make_gallery((page as u64) * 100 + i)).collect(),
        num_pages: total_pages,
        per_page: 25,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = MockClient;

    // 1. get_gallery
    let gallery = client.get_gallery(177013).await?;
    assert_eq!(gallery.id, 177013);
    assert_eq!(gallery.media_id, "123456789");
    assert_eq!(gallery.best_title(), "Test Gallery Title");
    assert_eq!(gallery.num_pages, 3);
    println!("[PASS] get_gallery: id={}, title={}", gallery.id, gallery.best_title());

    // 2. Gallery helpers
    let langs = gallery.languages();
    assert_eq!(langs.len(), 1);
    assert_eq!(langs[0].name, "english");
    println!("[PASS] languages: {:?}", langs.iter().map(|t| &t.name).collect::<Vec<_>>());

    let artists = gallery.artists();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].name, "artist_name");
    println!("[PASS] artists: {:?}", artists.iter().map(|t| &t.name).collect::<Vec<_>>());

    let tags = gallery.tags_by_type(&TagType::Tag);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "full color");
    println!("[PASS] tags_by_type(tag): {:?}", tags.iter().map(|t| &t.name).collect::<Vec<_>>());

    // 3. search
    let r = client.search("tag:\"full color\"", 1).await?;
    assert_eq!(r.result.len(), 3);
    assert_eq!(r.num_pages, 3);
    println!("[PASS] search: {} results, {} pages", r.result.len(), r.num_pages);

    // 4. tagged
    let r = client.tagged(12345, 2).await?;
    assert_eq!(r.result.len(), 3);
    assert_eq!(r.num_pages, 5);
    println!("[PASS] tagged: {} results, {} pages", r.result.len(), r.num_pages);

    // 5. all
    let all = client.all(1).await?;
    assert_eq!(all.result.len(), 3);
    assert_eq!(all.num_pages, 10);
    println!("[PASS] all: {} results, {} pages", all.result.len(), all.num_pages);

    // 6. CdnConfig
    let cdn = CdnConfig::default();
    assert_eq!(cdn.image_server(0), "https://i.nhentai.net");
    assert_eq!(cdn.thumb_server(0), "https://t.nhentai.net");
    assert_eq!(cdn.image_server(100), "https://i1.nhentai.net");
    println!("[PASS] CdnConfig: {} image, {} thumb", cdn.image_servers.len(), cdn.thumb_servers.len());

    // 7. URL builders
    assert_eq!(image_url::image_url("https://i.nhentai.net", "abc", 1, "jpg"), "https://i.nhentai.net/abc/1.jpg");
    assert_eq!(image_url::thumbnail_url("https://t.nhentai.net", "abc", 1), "https://t.nhentai.net/abc/1t.jpg");
    assert_eq!(image_url::cover_url("https://t.nhentai.net", "abc", "jpg"), "https://t.nhentai.net/abc/cover.jpg");
    println!("[PASS] URL builders");

    // 8. Dynamic CDN URLs
    assert_eq!(image_url::get_image_url(&cdn, "abc", 2, "png", 1), "https://i1.nhentai.net/abc/2.png");
    assert_eq!(image_url::get_thumbnail_url(&cdn, "abc", 3, 2), "https://t2.nhentai.net/abc/3t.jpg");
    assert_eq!(image_url::get_cover_url(&cdn, "abc", "png", 0), "https://t.nhentai.net/abc/cover.png");
    println!("[PASS] Dynamic CDN URLs");

    // 9. Gallery page helpers
    assert_eq!(image_url::gallery_page_url(&cdn, &gallery, 1, 0)?, "https://i.nhentai.net/123456789/1.jpg");
    assert_eq!(image_url::gallery_page_url(&cdn, &gallery, 2, 1)?, "https://i1.nhentai.net/123456789/2.png");
    assert_eq!(image_url::gallery_page_thumbnail_url(&cdn, &gallery, 1, 0)?, "https://t.nhentai.net/123456789/1t.jpg");
    assert_eq!(image_url::gallery_cover_url(&cdn, &gallery, 0), "https://t.nhentai.net/123456789/cover.jpg");
    println!("[PASS] Gallery page helpers");

    // 10. Invalid page error
    assert!(image_url::gallery_page_url(&cdn, &gallery, 999, 0).is_err());
    println!("[PASS] Invalid page => error");

    // 11. Serialize roundtrip
    let json = serde_json::to_string(&gallery).unwrap();
    let g2: Gallery = serde_json::from_str(&json).unwrap();
    assert_eq!(gallery.id, g2.id);
    assert_eq!(gallery.best_title(), g2.best_title());
    println!("[PASS] Serialize/Deserialize roundtrip: {} bytes", json.len());

    // 12. PaginatedResponse serialize
    let pj = serde_json::to_string(&all).unwrap();
    println!("[PASS] PaginatedResponse serialized: {} bytes", pj.len());

    println!("\nAll nh-api mock tests passed!");
    Ok(())
}