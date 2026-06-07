//! Web-based API test server for nH-Viewer.
//!
//! Integrates nh-api, nh-storage, and nh-download for full backend testing.
//!
//! Run with: `cargo run -p nh-tests --bin test_server`

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nh_api::endpoints::{DownloadFormat, GalleryEndpoints, Sort, TagSort};
use nh_api::{ClientConfig, NhClient, ProxyMode};
use nh_download::progress::ChannelReporter;
use nh_download::DownloadManager;
use nh_storage::settings::Settings;
use nh_storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    client: Arc<RwLock<Option<Arc<NhClient>>>>,
    proxy: Arc<RwLock<String>>,
    downloader: Arc<Mutex<Option<DownloadManager>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            proxy: Arc::new(RwLock::new(
                std::env::var("TEST_PROXY").unwrap_or_else(|_| "auto".into()),
            )),
            downloader: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_client(&self) -> Result<Arc<NhClient>, String> {
        let guard = self.client.read().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "Client not initialized. Click 'Apply' to connect.".to_string())
    }

    async fn get_proxy(&self) -> String {
        self.proxy.read().await.clone()
    }

    async fn init_client(&self, proxy: &str) -> Result<(), String> {
        // "disabled"/"none" → direct connection (no proxy)
        // "auto" or empty → use ProxyMode::System (reads HTTP_PROXY/HTTPS_PROXY/ALL_PROXY)
        // otherwise → use ProxyMode::Custom with the provided URL
        let proxy_mode = if proxy == "disabled" || proxy == "none" {
            ProxyMode::Disabled
        } else if proxy.is_empty() || proxy == "auto" {
            ProxyMode::System
        } else {
            ProxyMode::Custom(proxy.to_string())
        };

        // Persist the proxy setting BEFORE attempting connection,
        // so it survives even if the connection fails.
        {
            let mut p = self.proxy.write().await;
            *p = proxy.to_string();
        }

        // Keep old client & downloader alive during connection attempt.
        // Only replace them once the new connection succeeds.

        let config = ClientConfig::default().proxy_mode(proxy_mode);
        let client = NhClient::new(config)
            .await
            .map_err(|e| format!("Failed to create NhClient: {e}"))?;

        // Clone client for our own use (NhClient is Clone via Arc internally)
        let client_for_state = client.clone();

        // Initialize storage
        let settings = Settings::from_data_dir(std::path::PathBuf::from("target/test_data/webui"));
        let storage = Storage::init(settings)
            .await
            .map_err(|e| format!("Failed to init storage: {e}"))?;

        // Create progress channel
        let (reporter, _rx) = ChannelReporter::new();

        // Create download manager (takes ownership of client and storage)
        let mut downloader = DownloadManager::new(client, storage, reporter)
            .await
            .map_err(|e| format!("Failed to create download manager: {e}"))?;

        // Start worker pool (3 workers)
        downloader.start(3);

        // Store state
        {
            let mut g = self.client.write().await;
            *g = Some(Arc::new(client_for_state));
        }
        {
            let mut dl = self.downloader.lock().await;
            *dl = Some(downloader);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SafeQuery extractor
// ---------------------------------------------------------------------------

struct SafeQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for SafeQuery<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(SafeQuery(value)),
            Err(rejection) => {
                let err_msg = rejection.body_text();
                let body = ApiResponse::<serde_json::Value> {
                    success: false,
                    data: None,
                    error: Some(format!("Parameter error: {err_msg}")),
                    elapsed_ms: 0,
                };
                Err((StatusCode::BAD_REQUEST, Json(body)).into_response())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parameter types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GalleryQuery {
    id: u64,
    #[serde(default)]
    include: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default)]
    sort: String,
}

#[derive(Deserialize)]
struct TaggedQuery {
    tag_id: u64,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
    #[serde(default)]
    sort: String,
}

#[derive(Deserialize)]
struct FavoritesQuery {
    #[serde(default = "default_page")]
    page: u32,
}

#[derive(Deserialize)]
struct TagIdsQuery {
    ids: String,
}

#[derive(Deserialize)]
struct TagTypeQuery {
    tag_type: String,
    #[serde(default)]
    sort: String,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

#[derive(Deserialize)]
struct DownloadQuery {
    id: u64,
    #[serde(default)]
    format: String,
}

#[derive(Deserialize)]
struct ProxyRequest {
    proxy: String,
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

#[derive(Deserialize)]
struct DownloadStartQuery {
    id: u64,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    25
}

fn parse_sort(s: &str) -> Sort {
    match s {
        "popular" => Sort::Popular,
        "popular-today" => Sort::PopularToday,
        "popular-week" => Sort::PopularWeek,
        "popular-month" => Sort::PopularMonth,
        _ => Sort::Date,
    }
}

fn parse_tag_sort(s: &str) -> TagSort {
    match s {
        "name" => TagSort::Name,
        _ => TagSort::Popular,
    }
}

fn parse_download_format(s: &str) -> DownloadFormat {
    match s {
        "cbz" => DownloadFormat::Cbz,
        "torrent" => DownloadFormat::Torrent,
        _ => DownloadFormat::Zip,
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    elapsed_ms: u128,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T, elapsed: std::time::Duration) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            elapsed_ms: elapsed.as_millis(),
        }
    }
    fn err(msg: String, elapsed: std::time::Duration) -> ApiResponse<T> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(msg),
            elapsed_ms: elapsed.as_millis(),
        }
    }
}

macro_rules! timed {
    ($expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        let elapsed = start.elapsed();
        (result, elapsed)
    }};
}

type JsonResp = (StatusCode, Json<ApiResponse<serde_json::Value>>);

fn err_response(msg: String) -> JsonResp {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::<serde_json::Value>::err(
            msg,
            std::time::Duration::ZERO,
        )),
    )
}

// ---------------------------------------------------------------------------
// Proxy & status handlers
// ---------------------------------------------------------------------------

async fn handle_get_status(State(state): State<AppState>) -> JsonResp {
    let proxy = state.get_proxy().await;
    let has_client = state.client.read().await.is_some();
    let has_downloader = state.downloader.lock().await.is_some();
    let client_key = std::env::var("NH_API_KEY")
        .ok()
        .map(|k| {
            let len = k.len();
            if len > 8 {
                format!("{}...{}", &k[..4], &k[len - 4..])
            } else {
                "****".to_string()
            }
        })
        .unwrap_or_else(|| "Not set".to_string());

    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::json!({
                "proxy": proxy,
                "connected": has_client,
                "storage_ready": has_downloader,
                "downloader_ready": has_downloader,
                "api_key_masked": client_key,
            }),
            std::time::Duration::ZERO,
        )),
    )
}

async fn handle_set_proxy(
    State(state): State<AppState>,
    Json(req): Json<ProxyRequest>,
) -> JsonResp {
    let proxy = if req.proxy.trim().is_empty() {
        "auto".to_string()
    } else {
        req.proxy.trim().to_string()
    };

    let (result, elapsed) = timed!(state.init_client(&proxy).await);
    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::json!({ "proxy": proxy, "connected": true }),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::err(e, elapsed)),
        ),
    }
}

// ---------------------------------------------------------------------------
// nh-api handlers
// ---------------------------------------------------------------------------

async fn handle_warm_up(State(state): State<AppState>) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.warm_up().await);
    match result {
        Ok(status) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::json!({ "status": status }),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_get_gallery(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<GalleryQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let include = q.include.as_deref();
    let (result, elapsed) = timed!(client.get_gallery(q.id, include).await);
    match result {
        Ok(gallery) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&gallery).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_search(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<SearchQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let sort = parse_sort(&q.sort);
    let (result, elapsed) = timed!(client.search(&q.query, sort, q.page).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_popular(State(state): State<AppState>) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_popular_galleries().await);
    match result {
        Ok(list) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&list).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_random(State(state): State<AppState>) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_random_gallery().await);
    match result {
        Ok(id) => (
            StatusCode::OK,
            Json(ApiResponse::ok(serde_json::json!({ "id": id }), elapsed)),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_related(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<GalleryQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_related_galleries(q.id).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_tagged(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<TaggedQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let sort = parse_sort(&q.sort);
    let (result, elapsed) = timed!(
        client
            .get_galleries_tagged(q.tag_id, sort, q.page, q.per_page)
            .await
    );
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_favorites(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<FavoritesQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_favorites(q.page).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_add_favorite(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<GalleryQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.add_favorite(q.id).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_remove_favorite(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<GalleryQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.remove_favorite(q.id).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_tag_ids(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<TagIdsQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let ids: Vec<u64> = q
        .ids
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let (result, elapsed) = timed!(client.get_tags_by_ids(&ids).await);
    match result {
        Ok(tags) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&tags).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_tag_type(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<TagTypeQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let sort = parse_tag_sort(&q.sort);
    let (result, elapsed) = timed!(
        client
            .get_tags_by_type(&q.tag_type, sort, q.page, q.per_page)
            .await
    );
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_cdn_config(State(state): State<AppState>) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_cdn_config().await);
    match result {
        Ok(cdn) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&cdn).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_download_url(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<DownloadQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let format = parse_download_format(&q.format);
    let (result, elapsed) = timed!(client.download_gallery(q.id, format).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_galleries(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<FavoritesQuery>,
) -> JsonResp {
    let client = match state.get_client().await {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };
    let (result, elapsed) = timed!(client.get_galleries(q.page, 25).await);
    match result {
        Ok(resp) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&resp).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

// ---------------------------------------------------------------------------
// nh-storage handlers (access storage through downloader)
// ---------------------------------------------------------------------------

async fn handle_history(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<HistoryQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let storage = dl.storage();
    let pool = storage.db().pool();
    let offset = (q.page.saturating_sub(1)) * q.per_page;
    let (result, elapsed) = timed!(nh_storage::db::history::list(pool, q.per_page, offset).await);
    match result {
        Ok(entries) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&entries).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_search_history(State(state): State<AppState>) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let storage = dl.storage();
    let pool = storage.db().pool();
    let (result, elapsed) =
        timed!(nh_storage::db::gallery_cache::list_search_history(pool, 50).await);
    match result {
        Ok(entries) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&entries).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_local_favorites(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<HistoryQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let storage = dl.storage();
    let pool = storage.db().pool();
    let offset = (q.page.saturating_sub(1)) * q.per_page;
    let (result, elapsed) = timed!(nh_storage::db::favorites::list(pool, q.per_page, offset).await);
    match result {
        Ok(entries) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::to_value(&entries).unwrap(),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

// ---------------------------------------------------------------------------
// nh-download handlers
// ---------------------------------------------------------------------------

async fn handle_download_start(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<DownloadStartQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (result, elapsed) = timed!(dl.submit_gallery_download(q.id).await);
    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::json!({ "gallery_id": q.id, "status": "submitted" }),
                elapsed,
            )),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::err(e.to_string(), elapsed)),
        ),
    }
}

async fn handle_download_queue(State(state): State<AppState>) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (tasks, elapsed) = timed!(dl.task_snapshot().await);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::to_value(&tasks).unwrap(),
            elapsed,
        )),
    )
}

async fn handle_download_pause(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<DownloadStartQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (result, elapsed) = timed!(dl.pause(q.id).await);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::json!({ "paused": result }),
            elapsed,
        )),
    )
}

async fn handle_download_resume(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<DownloadStartQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (result, elapsed) = timed!(dl.resume(q.id).await);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::json!({ "resumed": result }),
            elapsed,
        )),
    )
}

async fn handle_download_cancel(
    State(state): State<AppState>,
    SafeQuery(q): SafeQuery<DownloadStartQuery>,
) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (result, elapsed) = timed!(dl.cancel(q.id).await);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::json!({ "cancelled": result }),
            elapsed,
        )),
    )
}

async fn handle_download_cancel_all(State(state): State<AppState>) -> JsonResp {
    let guard = state.downloader.lock().await;
    let dl = match guard.as_ref() {
        Some(d) => d,
        None => return err_response("Downloader not initialized.".into()),
    };
    let (_result, elapsed) = timed!(dl.cancel_all_and_clear().await);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(
            serde_json::json!({ "cancelled_all": true, "queue_cleared": true }),
            elapsed,
        )),
    )
}

// ---------------------------------------------------------------------------
// HTML page
// ---------------------------------------------------------------------------

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let state = AppState::new();

    // Auto-init with default proxy
    let default_proxy = state.get_proxy().await;
    match state.init_client(&default_proxy).await {
        Ok(()) => println!("✅ Client initialized with proxy: {default_proxy}"),
        Err(e) => println!("⚠️  Auto-init failed ({e}). Configure proxy in the UI."),
    }

    let app = Router::new()
        .route("/", get(index))
        // Status & proxy
        .route("/api/status", get(handle_get_status))
        .route("/api/proxy", post(handle_set_proxy))
        // nh-api endpoints
        .route("/api/warm_up", get(handle_warm_up))
        .route("/api/gallery", get(handle_get_gallery))
        .route("/api/search", get(handle_search))
        .route("/api/popular", get(handle_popular))
        .route("/api/random", get(handle_random))
        .route("/api/related", get(handle_related))
        .route("/api/tagged", get(handle_tagged))
        .route("/api/galleries", get(handle_galleries))
        .route("/api/favorites", get(handle_favorites))
        .route("/api/favorite/add", post(handle_add_favorite))
        .route("/api/favorite/remove", post(handle_remove_favorite))
        .route("/api/tags/ids", get(handle_tag_ids))
        .route("/api/tags/type", get(handle_tag_type))
        .route("/api/cdn", get(handle_cdn_config))
        .route("/api/download_url", get(handle_download_url))
        // nh-storage endpoints
        .route("/api/history", get(handle_history))
        .route("/api/search_history", get(handle_search_history))
        .route("/api/local_favorites", get(handle_local_favorites))
        // nh-download endpoints
        .route("/api/download/start", get(handle_download_start))
        .route("/api/download/queue", get(handle_download_queue))
        .route("/api/download/pause", get(handle_download_pause))
        .route("/api/download/resume", get(handle_download_resume))
        .route("/api/download/cancel", get(handle_download_cancel))
        .route("/api/download/cancel_all", post(handle_download_cancel_all))
        .with_state(state);

    let port = 3210;
    let addr = format!("127.0.0.1:{port}");
    println!("🚀 nH-Viewer Test Server starting on http://{addr}");
    println!("   Open http://{addr} in your browser");

    let _ = open::that(format!("http://{addr}"));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}
