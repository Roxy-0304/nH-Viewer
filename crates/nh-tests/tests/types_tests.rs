//! Unit tests for nh-api v2 data types and image URL generation.

use nh_api::image_url::{build_image_url, build_thumb_url};
use nh_api::types::{
    CdnConfig, GalleryDetailResponse, GalleryListItem, GalleryTitle, ImageFileType, TagResponse,
};

// =========================================================================
// ImageFileType tests
// =========================================================================

#[test]
fn image_file_type_as_extension() {
    assert_eq!(ImageFileType::Jpg.as_extension(), "jpg");
    assert_eq!(ImageFileType::Png.as_extension(), "png");
    assert_eq!(ImageFileType::Gif.as_extension(), "gif");
}

#[test]
fn image_file_type_display() {
    assert_eq!(format!("{}", ImageFileType::Jpg), "jpg");
    assert_eq!(format!("{}", ImageFileType::Png), "png");
    assert_eq!(format!("{}", ImageFileType::Gif), "gif");
}

#[test]
fn image_file_type_deserialize_from_json() {
    let jpg: ImageFileType = serde_json::from_str(r#""jpg""#).unwrap();
    let png: ImageFileType = serde_json::from_str(r#""png""#).unwrap();
    let gif: ImageFileType = serde_json::from_str(r#""gif""#).unwrap();

    assert!(matches!(jpg, ImageFileType::Jpg));
    assert!(matches!(png, ImageFileType::Png));
    assert!(matches!(gif, ImageFileType::Gif));
}

#[test]
fn image_file_type_serialize_to_json() {
    assert_eq!(
        serde_json::to_string(&ImageFileType::Jpg).unwrap(),
        r#""jpg""#
    );
    assert_eq!(
        serde_json::to_string(&ImageFileType::Png).unwrap(),
        r#""png""#
    );
    assert_eq!(
        serde_json::to_string(&ImageFileType::Gif).unwrap(),
        r#""gif""#
    );
}

#[test]
fn image_file_type_invalid_deserialize() {
    let result = serde_json::from_str::<ImageFileType>(r#""bmp""#);
    assert!(result.is_err());
}

// =========================================================================
// GalleryTitle tests
// =========================================================================

#[test]
fn title_best_prefers_pretty() {
    let title = GalleryTitle {
        english: "English Title".to_string(),
        japanese: Some("Japanese Title".to_string()),
        pretty: "Pretty Title".to_string(),
    };
    assert_eq!(title.best(), "Pretty Title");
}

#[test]
fn title_best_falls_back_to_english_when_pretty_empty() {
    let title = GalleryTitle {
        english: "English Title".to_string(),
        japanese: Some("Japanese Title".to_string()),
        pretty: String::new(),
    };
    assert_eq!(title.best(), "English Title");
}

#[test]
fn title_deserialize_from_json() {
    let json = r#"{
        "english": "Test Title",
        "japanese": "テスト",
        "pretty": "Test Title (Pretty)"
    }"#;
    let title: GalleryTitle = serde_json::from_str(json).unwrap();
    assert_eq!(title.english, "Test Title");
    assert_eq!(title.japanese.as_deref(), Some("テスト"));
    assert_eq!(title.pretty, "Test Title (Pretty)");
}

// =========================================================================
// TagResponse tests
// =========================================================================

#[test]
fn tag_deserialize_from_json() {
    let json = r#"{
        "id": 12345,
        "name": "test_tag",
        "type": "tag",
        "slug": "test_tag",
        "url": "/tags/test_tag/",
        "count": 1000
    }"#;
    let tag: TagResponse = serde_json::from_str(json).unwrap();
    assert_eq!(tag.id, 12345);
    assert_eq!(tag.name, "test_tag");
    assert_eq!(tag.tag_type, "tag");
    assert_eq!(tag.slug, "test_tag");
    assert_eq!(tag.url, "/tags/test_tag/");
    assert_eq!(tag.count, 1000);
}

#[test]
fn tag_deserialize_with_optional_fields() {
    let json = r#"{
        "id": 1,
        "name": "community_tag",
        "type": "artist",
        "slug": "community_tag",
        "url": "/artists/community_tag/",
        "count": 500,
        "description": "A community tag",
        "is_community": true,
        "pending_describe_id": null
    }"#;
    let tag: TagResponse = serde_json::from_str(json).unwrap();
    assert_eq!(tag.description.as_deref(), Some("A community tag"));
    assert_eq!(tag.is_community, Some(true));
}

// =========================================================================
// GalleryListItem tests
// =========================================================================

#[test]
fn gallery_list_item_deserialize() {
    let json = r#"{
        "id": 1,
        "media_id": "abc123",
        "english_title": "Test Gallery",
        "japanese_title": null,
        "thumbnail": "/galleries/1/thumb.jpg",
        "thumbnail_width": 350,
        "thumbnail_height": 500,
        "num_pages": 10,
        "num_favorites": 5,
        "tag_ids": [1, 2, 3],
        "blacklisted": false
    }"#;
    let item: GalleryListItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.id, 1);
    assert_eq!(item.media_id, "abc123");
    assert_eq!(item.english_title, "Test Gallery");
    assert_eq!(item.best_title(), "Test Gallery");
    assert_eq!(item.num_pages, 10);
    assert_eq!(item.num_favorites, 5);
    assert_eq!(item.tag_ids, vec![1, 2, 3]);
}

// =========================================================================
// GalleryDetailResponse tests
// =========================================================================

#[test]
fn gallery_detail_deserialize_full() {
    let json = r#"{
        "id": 177013,
        "media_id": "987539",
        "title": {
            "english": "Metamorphosis",
            "japanese": "変身",
            "pretty": "Metamorphosis"
        },
        "cover": {
            "path": "/galleries/987539/cover.jpg",
            "width": 350,
            "height": 500
        },
        "thumbnail": {
            "path": "/galleries/987539/thumb.jpg",
            "width": 350,
            "height": 500
        },
        "scanlator": "",
        "upload_date": 1234567890,
        "tags": [
            {"id": 1, "name": "big breasts", "type": "tag", "slug": "big-breasts", "url": "/tags/big-breasts/", "count": 100000}
        ],
        "num_pages": 225,
        "num_favorites": 50000,
        "pages": [
            {"number": 1, "path": "/galleries/987539/1.jpg", "width": 1280, "height": 1800, "thumbnail": "/galleries/987539/t1.jpg", "thumbnail_width": 350, "thumbnail_height": 500},
            {"number": 2, "path": "/galleries/987539/2.jpg", "width": 1280, "height": 1800, "thumbnail": "/galleries/987539/t2.jpg", "thumbnail_width": 350, "thumbnail_height": 500}
        ],
        "comments": null,
        "comment_count": null,
        "related": null,
        "is_favorited": null,
        "suggestions": null
    }"#;
    let gallery: GalleryDetailResponse = serde_json::from_str(json).unwrap();
    assert_eq!(gallery.id, 177013);
    assert_eq!(gallery.media_id, "987539");
    assert_eq!(gallery.title.best(), "Metamorphosis");
    assert_eq!(gallery.num_pages, 225);
    assert_eq!(gallery.num_favorites, 50000);
    assert_eq!(gallery.tags.len(), 1);
    assert_eq!(gallery.tags[0].name, "big breasts");
    assert_eq!(gallery.pages.len(), 2);
    assert_eq!(gallery.pages[0].path, "/galleries/987539/1.jpg");
    assert_eq!(gallery.cover.path, "/galleries/987539/cover.jpg");
}

#[test]
fn gallery_detail_deserialize_minimal() {
    let json = r#"{
        "id": 1,
        "media_id": "m1",
        "title": {"english": "Test", "pretty": "Test"},
        "cover": {"path": "/g/m1/c.jpg", "width": 350, "height": 500},
        "thumbnail": {"path": "/g/m1/t.jpg", "width": 350, "height": 500},
        "upload_date": 0,
        "num_pages": 1,
        "num_favorites": 0
    }"#;
    let gallery: GalleryDetailResponse = serde_json::from_str(json).unwrap();
    assert_eq!(gallery.id, 1);
    assert!(gallery.tags.is_empty());
    assert!(gallery.pages.is_empty());
    assert!(gallery.comments.is_none());
}

// =========================================================================
// Image URL tests (v2: path + CDN server)
// =========================================================================

#[test]
fn build_image_url_generation() {
    let cdn = CdnConfig::default();
    let path = "/galleries/987539/1.jpg";
    let url = build_image_url(&cdn, path, 0);
    assert!(url.contains("nhentai.net"));
    assert!(url.ends_with("/galleries/987539/1.jpg"));
}

#[test]
fn build_thumb_url_generation() {
    let cdn = CdnConfig::default();
    let path = "/galleries/987539/t1.jpg";
    let url = build_thumb_url(&cdn, path, 0);
    assert!(url.contains("nhentai.net"));
    assert!(url.ends_with("/galleries/987539/t1.jpg"));
}

// =========================================================================
// CdnConfig tests
// =========================================================================

#[test]
fn cdn_config_default_has_servers() {
    let cdn = CdnConfig::default();
    assert!(!cdn.image_servers.is_empty());
    assert!(!cdn.thumb_servers.is_empty());
}

#[test]
fn cdn_config_server_wraps_around() {
    let cdn = CdnConfig::default();
    let len = cdn.image_servers.len();
    let url1 = cdn.image_server(0);
    let url_wrap = cdn.image_server(len);
    assert_eq!(url1, url_wrap);
}