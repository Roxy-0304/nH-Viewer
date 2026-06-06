use once_cell::sync::OnceCell;
use tokio::sync::OnceCell as AsyncOnceCell;

use nh_api::endpoints::GalleryEndpoints;
use nh_api::image_url::{get_cover_url, get_thumbnail_url};
use nh_api::{Gallery, NhClient};
use nh_storage::repository::GalleryRepository;
pub use nh_storage::Storage;

// ---------------------------------------------------------------------------
// Global singletons (initialised lazily)
// ---------------------------------------------------------------------------

static API_CLIENT: AsyncOnceCell<NhClient> = AsyncOnceCell::const_new();
static STORAGE: OnceCell<Storage> = OnceCell::new();

/// Initialise the bridge layer — call once from Dart during app startup.
pub async fn init_bridge(storage: Storage) {
    // Store the storage instance (blocking is fine — it runs once).
    STORAGE
        .set(storage)
        .expect("init_bridge has already been called");

    // Lazily create the API client.
    API_CLIENT
        .get_or_init(|| async {
            NhClient::with_api_key(None)
                .await
                .expect("failed to create NhClient")
        })
        .await;
}

/// Convenience: get the global Storage handle.
fn storage() -> &'static Storage {
    STORAGE.get().expect("init_bridge has not been called yet")
}

/// Convenience: get the global NhClient handle.
fn api_client() -> &'static NhClient {
    API_CLIENT
        .get()
        .expect("API client not initialised — call init_bridge first")
}

// ---------------------------------------------------------------------------
// Bridge DTOs — lightweight types that FRB can pass to Dart
// ---------------------------------------------------------------------------

/// Full gallery info exposed to Dart.
#[derive(Debug, Clone)]
pub struct GalleryInfo {
    pub id: u64,
    pub media_id: String,
    pub title_en: Option<String>,
    pub title_jp: Option<String>,
    pub title_pretty: Option<String>,
    pub num_pages: u32,
    pub num_favorites: u32,
    pub cover_ext: Option<String>,
    pub cover_url: Option<String>,
    pub thumbnail_url: Option<String>,
}

/// Lightweight preview for list views.
#[derive(Debug, Clone)]
pub struct GalleryPreviewInfo {
    pub id: u64,
    pub title: String,
    pub num_pages: u32,
    pub cover_ext: Option<String>,
    pub cover_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn gallery_to_info(g: &Gallery) -> GalleryInfo {
    let cdn = API_CLIENT.get().map(|c| c.cdn_config());
    let (cover_url, thumbnail_url) = match cdn {
        Some(cdn) => {
            let cover_ext = g.images.cover.as_extension();
            let cover = get_cover_url(cdn, &g.media_id, cover_ext, 0);
            let thumb = get_thumbnail_url(cdn, &g.media_id, 1, 0);
            (Some(cover), Some(thumb))
        }
        None => (None, None),
    };

    GalleryInfo {
        id: g.id,
        media_id: g.media_id.clone(),
        title_en: g.title.english.clone(),
        title_jp: g.title.japanese.clone(),
        title_pretty: g.title.pretty.clone(),
        num_pages: g.num_pages,
        num_favorites: g.num_favorites,
        cover_ext: Some(g.images.cover.as_extension().to_string()),
        cover_url,
        thumbnail_url,
    }
}

fn cached_to_info(c: &nh_storage::db::gallery_cache::CachedGallery) -> GalleryInfo {
    GalleryInfo {
        id: c.id,
        media_id: c.media_id.clone(),
        title_en: c.title_en.clone(),
        title_jp: c.title_jp.clone(),
        title_pretty: c.title_pretty.clone(),
        num_pages: c.num_pages,
        num_favorites: c.num_favorites,
        cover_ext: c.cover_ext.clone(),
        cover_url: None,
        thumbnail_url: None,
    }
}

#[allow(dead_code)]
fn preview_to_info(p: &nh_storage::db::gallery_cache::GalleryPreview) -> GalleryPreviewInfo {
    GalleryPreviewInfo {
        id: p.id,
        title: p.title.clone(),
        num_pages: p.num_pages,
        cover_ext: p.cover_ext.clone(),
        cover_url: None,
    }
}

// ---------------------------------------------------------------------------
// Public API exposed to Dart
// ---------------------------------------------------------------------------

/// Fetch a gallery by ID.
///
/// Strategy: look in local cache first; on miss, fetch from the remote API
/// and persist the result for next time.
#[flutter_rust_bridge::frb]
pub async fn nh_get_gallery(id: u64) -> anyhow::Result<GalleryInfo> {
    let store = storage();

    // 1. Try local cache
    if let Some(cached) = store.get_gallery(id).await? {
        // If we have full raw_json, we can fill in image URLs
        let mut info = cached_to_info(&cached);

        // Try to fill image URLs from CDN config if available
        if let Some(client) = API_CLIENT.get() {
            if let Some(ref raw) = cached.raw_json {
                if let Ok(gallery) = serde_json::from_str::<Gallery>(raw) {
                    let cdn = client.cdn_config();
                    let cover_ext = gallery.images.cover.as_extension();
                    info.cover_url = Some(get_cover_url(cdn, &gallery.media_id, cover_ext, 0));
                    info.thumbnail_url =
                        Some(get_thumbnail_url(cdn, &gallery.media_id, 1, 0));
                }
            }
        }
        return Ok(info);
    }

    // 2. Fetch from API
    let client = api_client();
    let gallery = client.get_gallery(id).await?;

    // 3. Cache locally for next time
    let cached = nh_storage::db::gallery_cache::CachedGallery {
        id: gallery.id,
        media_id: gallery.media_id.clone(),
        title_en: gallery.title.english.clone(),
        title_jp: gallery.title.japanese.clone(),
        title_pretty: gallery.title.pretty.clone(),
        num_pages: gallery.num_pages,
        num_favorites: gallery.num_favorites,
        cover_ext: Some(gallery.images.cover.as_extension().to_string()),
        tags_json: Some(serde_json::to_string(&gallery.tags).unwrap_or_default()),
        raw_json: Some(serde_json::to_string(&gallery).unwrap_or_default()),
        cached_at: 0, // will be filled by the DB
    };
    let _ = store.save_gallery(&cached).await; // best-effort

    Ok(gallery_to_info(&gallery))
}

/// Search galleries by query string.
///
/// Fetches results from the remote API.
#[flutter_rust_bridge::frb]
pub async fn nh_search_galleries(
    query: String,
    page: u32,
) -> anyhow::Result<Vec<GalleryPreviewInfo>> {
    let client = api_client();
    let result = client.search(&query, page).await?;

    let previews: Vec<GalleryPreviewInfo> = result
        .result
        .iter()
        .map(|g| {
            let cdn = client.cdn_config();
            let cover_ext = g.images.cover.as_extension();
            GalleryPreviewInfo {
                id: g.id,
                title: g.title.best().to_string(),
                num_pages: g.num_pages,
                cover_ext: Some(cover_ext.to_string()),
                cover_url: Some(get_cover_url(cdn, &g.media_id, cover_ext, 0)),
            }
        })
        .collect();

    // Record search in history (best-effort)
    let _ = storage()
        .record_search(&query, previews.len() as u32)
        .await;

    Ok(previews)
}