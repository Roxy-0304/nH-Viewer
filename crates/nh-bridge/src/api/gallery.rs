use std::sync::Arc;

use once_cell::sync::OnceCell;
use tokio::sync::{OnceCell as AsyncOnceCell, RwLock};

use nh_api::client::ClientConfig;
use nh_api::endpoints::{GalleryEndpoints, Sort};
use nh_api::image_url::{build_image_url, build_thumb_url};
use nh_api::{GalleryDetailResponse, GalleryListItem, NhClient};
use nh_storage::db::gallery_cache;
pub use nh_storage::Storage;

// ---------------------------------------------------------------------------
// Global singletons (initialised lazily)
// ---------------------------------------------------------------------------

static API_CLIENT: AsyncOnceCell<Arc<RwLock<NhClient>>> = AsyncOnceCell::const_new();
static STORAGE: OnceCell<Storage> = OnceCell::new();

/// Initialise the bridge layer — call once from Dart during app startup.
pub async fn init_bridge(storage: Storage) -> anyhow::Result<()> {
    init_bridge_with_config(storage, ClientConfig::default()).await
}

/// Initialise the bridge layer with a custom `ClientConfig`.
///
/// This function is **idempotent** — calling it a second time is a no-op that
/// returns `Ok(())`, so callers don't need to track whether initialization has
/// already happened.
#[flutter_rust_bridge::frb(ignore)]
pub async fn init_bridge_with_config(storage: Storage, config: ClientConfig) -> anyhow::Result<()> {
    // Idempotent: if already initialised, silently return.
    if STORAGE.get().is_some() {
        return Ok(());
    }

    STORAGE
        .set(storage)
        .map_err(|_| anyhow::anyhow!("init_bridge has already been called"))?;

    API_CLIENT
        .get_or_init(|| async {
            match NhClient::new(config.clone()).await {
                Ok(client) => Arc::new(RwLock::new(client)),
                Err(e) => panic!("failed to create NhClient: {e}"),
            }
        })
        .await;

    Ok(())
}

fn storage() -> anyhow::Result<&'static Storage> {
    STORAGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("init_bridge has not been called yet"))
}

async fn api_client() -> anyhow::Result<tokio::sync::RwLockReadGuard<'static, NhClient>> {
    let cell = API_CLIENT
        .get()
        .ok_or_else(|| anyhow::anyhow!("API client not initialised — call init_bridge first"))?;
    Ok(cell.read().await)
}

// ---------------------------------------------------------------------------
// API Key management
// ---------------------------------------------------------------------------

#[flutter_rust_bridge::frb]
pub async fn nh_set_api_key(api_key: String) -> anyhow::Result<()> {
    let cell = API_CLIENT
        .get()
        .ok_or_else(|| anyhow::anyhow!("API client not initialised — call init_bridge first"))?;

    let config = ClientConfig::new(&api_key);
    let new_client = NhClient::new(config).await?;

    let mut guard = cell.write().await;
    *guard = new_client;

    Ok(())
}

#[flutter_rust_bridge::frb]
pub async fn nh_get_api_key() -> anyhow::Result<String> {
    let client = api_client().await?;
    Ok(client.api_key().unwrap_or_default().to_string())
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
    pub cover_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub tags: Vec<TagInfo>,
}

/// Lightweight preview for list views.
#[derive(Debug, Clone)]
pub struct GalleryPreviewInfo {
    pub id: u64,
    pub title: String,
    pub num_pages: u32,
    pub cover_url: Option<String>,
}

/// Tag info for Dart.
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub id: u64,
    pub tag_type: String,
    pub name: String,
    pub url: String,
    pub count: u64,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn gallery_detail_to_info(
    g: &GalleryDetailResponse,
    cdn: Option<&nh_api::types::CdnConfig>,
) -> GalleryInfo {
    let (cover_url, thumbnail_url) = match cdn {
        Some(cdn) => {
            let server_idx = g.id as usize;
            let cover = build_image_url(cdn, &g.cover.path, server_idx);
            let thumb = build_thumb_url(cdn, &g.thumbnail.path, server_idx);
            (Some(cover), Some(thumb))
        }
        None => (None, None),
    };

    let tags: Vec<TagInfo> = g
        .tags
        .iter()
        .map(|t| TagInfo {
            id: t.id,
            tag_type: t.tag_type.clone(),
            name: t.name.clone(),
            url: t.url.clone(),
            count: t.count,
        })
        .collect();

    GalleryInfo {
        id: g.id,
        media_id: g.media_id.clone(),
        title_en: Some(g.title.english.clone()),
        title_jp: g.title.japanese.clone(),
        title_pretty: Some(g.title.pretty.clone()),
        num_pages: g.num_pages,
        num_favorites: g.num_favorites,
        cover_url,
        thumbnail_url,
        tags,
    }
}

fn gallery_list_to_preview(
    g: &GalleryListItem,
    cdn: Option<&nh_api::types::CdnConfig>,
) -> GalleryPreviewInfo {
    let cover_url = cdn.map(|c| {
        let server_idx = g.id as usize;
        build_thumb_url(c, &g.thumbnail, server_idx)
    });

    GalleryPreviewInfo {
        id: g.id,
        title: g.best_title().to_string(),
        num_pages: g.num_pages,
        cover_url,
    }
}

// ---------------------------------------------------------------------------
// Public API exposed to Dart
// ---------------------------------------------------------------------------

/// Warm up the connection by visiting the main page.
#[flutter_rust_bridge::frb]
pub async fn nh_warm_up() -> anyhow::Result<u16> {
    let client = api_client().await?;
    let status = client.warm_up().await?;
    Ok(status)
}

/// Fetch a gallery by ID (cache-first strategy).
#[flutter_rust_bridge::frb]
pub async fn nh_get_gallery(id: u64) -> anyhow::Result<GalleryInfo> {
    let store = storage()?;
    let pool = store.db().pool();
    let client = api_client().await?;

    // 1. Try local cache — read the raw JSON string
    if let Some(raw) = gallery_cache::get_gallery_raw(pool, id).await? {
        if let Ok(gallery) = serde_json::from_str::<GalleryDetailResponse>(&raw) {
            let cdn = Some(client.cdn_config());
            let info = gallery_detail_to_info(&gallery, cdn);
            return Ok(info);
        }
    }

    // 2. Fetch from API
    let gallery = client.get_gallery(id, None).await?;
    let cdn = Some(client.cdn_config());

    // 3. Cache the raw JSON for next time (best-effort)
    let raw_json = serde_json::to_string(&gallery).unwrap_or_default();
    let _ = gallery_cache::upsert_gallery(pool, id, &raw_json).await;

    Ok(gallery_detail_to_info(&gallery, cdn))
}

/// Search galleries by query string.
#[flutter_rust_bridge::frb]
pub async fn nh_search_galleries(
    query: String,
    page: u32,
) -> anyhow::Result<Vec<GalleryPreviewInfo>> {
    let client = api_client().await?;
    let result = client.search(&query, Sort::Date, page).await?;

    let cdn = client.cdn_config();
    let previews: Vec<GalleryPreviewInfo> = result
        .result
        .iter()
        .map(|g| gallery_list_to_preview(g, Some(cdn)))
        .collect();

    // Record search in history (best-effort)
    if let Ok(store) = storage() {
        let pool = store.db().pool();
        let _ = gallery_cache::record_search(pool, &query, previews.len() as u32).await;
    }

    Ok(previews)
}

/// Get popular galleries.
#[flutter_rust_bridge::frb]
pub async fn nh_get_popular() -> anyhow::Result<Vec<GalleryPreviewInfo>> {
    let client = api_client().await?;
    let result = client.get_popular_galleries().await?;

    let cdn = client.cdn_config();
    let previews: Vec<GalleryPreviewInfo> = result
        .iter()
        .map(|g| gallery_list_to_preview(g, Some(cdn)))
        .collect();

    Ok(previews)
}

/// Get a random gallery ID.
#[flutter_rust_bridge::frb]
pub async fn nh_get_random() -> anyhow::Result<u64> {
    let client = api_client().await?;
    let id = client.get_random_gallery().await?;
    Ok(id)
}
