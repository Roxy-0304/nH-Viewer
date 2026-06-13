use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use tracing::{debug, warn};

use crate::endpoints::{DownloadFormat, GalleryEndpoints, Sort, TagSort};
use crate::error::{Error, Result};
use crate::types::{
    CdnConfig, DownloadResponse, FavoriteResponse, GalleryDetailResponse, GalleryListItem,
    PaginatedResponse, RelatedGalleriesResponse, TagResponse,
};

/// Full browser-like User-Agent string (Chrome on Windows).
/// Shared between `NhClient` and download workers for consistency.
pub const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// Detect the system proxy setting.
///
/// - **Windows**: reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
///   → `ProxyServer` (e.g. `127.0.0.1:7897`) only when `ProxyEnable` is 1.
/// - **macOS / Linux / Android**: reads `HTTP_PROXY`, `HTTPS_PROXY`, or `ALL_PROXY` env vars.
///
/// Returns `Some(proxy_url)` if a proxy is configured, `None` otherwise.
fn detect_system_proxy() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
        if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_READ) {
            // Check if proxy is enabled
            let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
            if enabled != 1 {
                debug!("Windows system proxy is disabled (ProxyEnable=0)");
                return None;
            }
            // Read proxy server address
            if let Ok(proxy_server) = key.get_value::<String, _>("ProxyServer") {
                let server = proxy_server.trim().to_string();
                if !server.is_empty() {
                    // Ensure it has a scheme prefix for reqwest
                    let url = if server.contains("://") {
                        server
                    } else {
                        format!("http://{}", server)
                    };
                    debug!("Windows system proxy detected: {}", url);
                    return Some(url);
                }
            }
            debug!("Windows system proxy enabled but ProxyServer is empty");
        }
        None
    }

    #[cfg(not(windows))]
    {
        // Check standard environment variables
        for var in &[
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            if let Ok(val) = std::env::var(var) {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    debug!("System proxy detected from env {}: {}", var, val);
                    return Some(val);
                }
            }
        }
        None
    }
}

/// Proxy mode for HTTP requests.
#[derive(Debug, Clone, Default)]
pub enum ProxyMode {
    /// Do not use any proxy.
    #[default]
    Disabled,
    /// Use system proxy (reads HTTP_PROXY / HTTPS_PROXY / ALL_PROXY env vars).
    System,
    /// Use a custom proxy URL (e.g. "http://127.0.0.1:7897", "socks5://...").
    Custom(String),
}

impl ProxyMode {
    /// Try to build a `reqwest::Proxy` from this mode.
    /// Returns `Ok(None)` when disabled, `Ok(Some(...))` when configured,
    /// or `Err` if the proxy URL is invalid.
    pub fn to_reqwest_proxy(&self) -> crate::error::Result<Option<reqwest::Proxy>> {
        match self {
            Self::Disabled => Ok(None),
            Self::System => {
                // Cross-platform system proxy detection:
                // - Windows: reads registry Internet Settings (ProxyServer)
                // - macOS/Linux/Android: reads HTTP_PROXY/HTTPS_PROXY/ALL_PROXY env vars
                if let Some(proxy_url) = detect_system_proxy() {
                    let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| {
                        crate::error::Error::CdnConfigFetch {
                            reason: format!("invalid system proxy URL '{}': {e}", proxy_url),
                        }
                    })?;
                    Ok(Some(proxy))
                } else {
                    // No system proxy configured — connect directly
                    Ok(None)
                }
            }
            Self::Custom(url) => {
                let proxy =
                    reqwest::Proxy::all(url).map_err(|e| crate::error::Error::CdnConfigFetch {
                        reason: format!("invalid proxy URL: {e}"),
                    })?;
                Ok(Some(proxy))
            }
        }
    }
}

/// Configuration for the nhentai API client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL for the API (e.g. "https://nhentai.net")
    pub base_url: String,
    /// API key for authorization
    pub api_key: Option<String>,
    /// Request timeout
    pub timeout: Duration,
    /// Maximum number of retries
    pub max_retries: u32,
    /// Base delay for exponential backoff
    pub retry_delay: Duration,
    /// Whether to dynamically fetch CDN config on init
    pub enable_dynamic_cdn: bool,
    /// Proxy mode for all HTTP requests
    pub proxy_mode: ProxyMode,
    /// Whether to enable cookie jar for session persistence
    pub cookie_store: bool,
    /// Path to cookie file for persistence across restarts (requires cookie_store = true)
    pub cookie_file: Option<std::path::PathBuf>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://nhentai.net".to_string(),
            api_key: std::env::var("NH_API_KEY").ok().filter(|s| !s.is_empty()),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            enable_dynamic_cdn: true,
            proxy_mode: ProxyMode::Disabled,
            cookie_store: true,
            cookie_file: None,
        }
    }
}

impl ClientConfig {
    /// Create a new configuration with an API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: Some(api_key.into()),
            ..Default::default()
        }
    }

    /// Set the request timeout
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of retries
    pub const fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the base delay for exponential backoff
    pub const fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }

    /// Enable or disable dynamic CDN configuration
    pub const fn dynamic_cdn(mut self, enable: bool) -> Self {
        self.enable_dynamic_cdn = enable;
        self
    }

    /// Set the proxy mode
    pub fn proxy_mode(mut self, mode: ProxyMode) -> Self {
        self.proxy_mode = mode;
        self
    }

    /// Convenience: set a custom proxy URL
    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy_mode = ProxyMode::Custom(proxy.into());
        self
    }

    /// Enable or disable cookie jar
    pub const fn cookies(mut self, enable: bool) -> Self {
        self.cookie_store = enable;
        self
    }

    /// Set cookie file path for persistence
    pub fn cookie_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.cookie_file = Some(path.into());
        self
    }
}

/// nhentai API client
#[derive(Debug, Clone)]
pub struct NhClient {
    client: Client,
    config: ClientConfig,
    cdn_config: Arc<CdnConfig>,
}

impl NhClient {
    /// Build browser-like default headers for the HTTP client.
    fn build_headers(config: &ClientConfig) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(BROWSER_USER_AGENT),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("*/*"),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );
        headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_static("https://nhentai.net/"),
        );

        if let Some(ref api_key) = config.api_key {
            let auth_value = format!("Key {}", api_key);
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_value)
                    .map_err(|_| Error::InvalidApiKey)?,
            );
        }

        Ok(headers)
    }

    /// Create a new client with the given configuration.
    ///
    /// If `enable_dynamic_cdn` is true in the config, this will attempt to
    /// fetch CDN configuration from `/api/v2/cdn`. If the fetch fails,
    /// a default fallback configuration is used.
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let mut builder = Client::builder().timeout(config.timeout);

        // Proxy
        if let Some(proxy) = config.proxy_mode.to_reqwest_proxy()? {
            builder = builder.proxy(proxy);
        }

        // Cookie jar
        if config.cookie_store {
            builder = builder.cookie_provider(Arc::new(reqwest::cookie::Jar::default()));
        }

        let headers = Self::build_headers(&config)?;

        let client = builder.default_headers(headers).build()?;

        let cdn_config = if config.enable_dynamic_cdn {
            match Self::fetch_cdn_config_inner(&client, &config).await {
                Ok(cdn) => {
                    debug!(
                        "CDN config fetched successfully: {} image servers, {} thumb servers",
                        cdn.image_servers.len(),
                        cdn.thumb_servers.len()
                    );
                    Arc::new(cdn)
                }
                Err(e) => {
                    warn!("Failed to fetch CDN config, using defaults: {}", e);
                    Arc::new(CdnConfig::default())
                }
            }
        } else {
            Arc::new(CdnConfig::default())
        };

        Ok(Self {
            client,
            config,
            cdn_config,
        })
    }

    /// Create a client with default configuration and optional API key.
    /// This is a convenience method that fetches CDN config on init.
    pub async fn with_api_key(api_key: Option<String>) -> Result<Self> {
        let config = ClientConfig {
            api_key,
            ..Default::default()
        };
        Self::new(config).await
    }

    /// Create a client synchronously with static CDN config (no network call).
    /// Use this when you want to avoid the async CDN fetch at construction time.
    pub fn new_static(config: ClientConfig) -> Result<Self> {
        let mut builder = Client::builder().timeout(config.timeout);

        if let Some(proxy) = config.proxy_mode.to_reqwest_proxy()? {
            builder = builder.proxy(proxy);
        }

        if config.cookie_store {
            builder = builder.cookie_provider(Arc::new(reqwest::cookie::Jar::default()));
        }

        let headers = Self::build_headers(&config)?;
        let client = builder.default_headers(headers).build()?;

        Ok(Self {
            client,
            config,
            cdn_config: Arc::new(CdnConfig::default()),
        })
    }

    /// Fetch CDN configuration from the API (internal helper)
    async fn fetch_cdn_config_inner(client: &Client, config: &ClientConfig) -> Result<CdnConfig> {
        let url = format!("{}/api/v2/cdn", config.base_url);
        debug!("Fetching CDN config: {}", url);

        let response = client.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::CdnConfigFetch {
                reason: format!("HTTP {} - {}", status, body),
            });
        }

        let cdn: CdnConfig = response.json().await?;
        Ok(cdn)
    }

    /// Refresh the CDN configuration from the server.
    /// Returns the new config, and updates the client's stored config.
    pub async fn refresh_cdn_config(&mut self) -> Result<CdnConfig> {
        let cdn = Self::fetch_cdn_config_inner(&self.client, &self.config).await?;
        self.cdn_config = Arc::new(cdn.clone());
        Ok(cdn)
    }

    /// Get the current CDN configuration
    pub fn cdn_config(&self) -> &CdnConfig {
        &self.cdn_config
    }

    /// Get the current API key (if set)
    pub fn api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    /// Get the current proxy mode
    pub const fn proxy_mode(&self) -> &ProxyMode {
        &self.config.proxy_mode
    }

    /// Build a `reqwest::Proxy` from the current proxy configuration.
    /// Useful for other modules that need to create their own `reqwest::Client`.
    pub fn build_reqwest_proxy(&self) -> Result<Option<reqwest::Proxy>> {
        self.config.proxy_mode.to_reqwest_proxy()
    }

    /// Warm up: visit the main page to obtain Cloudflare cookies (cf_clearance).
    /// Returns the HTTP status code of the response.
    pub async fn warm_up(&self) -> Result<u16> {
        let url = format!("{}/", self.config.base_url);
        debug!("Warming up: {}", url);

        let response = self.client.get(&url).send().await?;
        let status = response.status().as_u16();
        debug!("Warm-up response: HTTP {}", status);
        Ok(status)
    }

    /// Execute a request with retry logic.
    ///
    /// Handles exponential backoff, rate limiting, and error responses uniformly.
    async fn request_with_retry<F, Fut>(
        &self,
        method: &str,
        url: &str,
        make_request: F,
    ) -> Result<Response>
    where
        F: Fn(&Client, &str) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<Response, reqwest::Error>>,
    {
        let mut skip_delay = false;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 && !skip_delay {
                let delay = self.config.retry_delay * 2u32.pow(attempt - 1);
                debug!("Retry {} attempt {} after {:?}", method, attempt, delay);
                tokio::time::sleep(delay).await;
            }
            // Reset the flag so subsequent iterations use normal backoff
            skip_delay = false;

            match make_request(&self.client, url).await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return Ok(response);
                    }

                    match status {
                        StatusCode::TOO_MANY_REQUESTS => {
                            let retry_after = response
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.parse::<u64>().ok())
                                .map(Duration::from_secs);

                            warn!(
                                "Rate limited on {} request, retry after {:?}",
                                method, retry_after
                            );

                            if attempt < self.config.max_retries {
                                let delay = retry_after
                                    .unwrap_or_else(|| self.config.retry_delay * 2u32.pow(attempt));
                                // Sleep for the 429 delay and skip the loop-top
                                // backoff on the next iteration to avoid double sleep.
                                tokio::time::sleep(delay).await;
                                skip_delay = true;
                                continue;
                            }

                            return Err(Error::RateLimited { retry_after });
                        }
                        _ => {
                            let body = response.text().await.unwrap_or_default();
                            return Err(Error::HttpError {
                                status: status.as_u16(),
                                body,
                            });
                        }
                    }
                }
                Err(e) => {
                    warn!("{} request failed: {}", method, e);
                    if attempt == self.config.max_retries {
                        return Err(Error::Http(e));
                    }
                }
            }
        }

        Err(Error::RetryFailed {
            attempts: self.config.max_retries + 1,
        })
    }

    /// Execute a GET request with retry logic
    async fn get(&self, url: &str) -> Result<Response> {
        self.request_with_retry("GET", url, |client, url| client.get(url).send())
            .await
    }

    /// Execute a POST request with retry logic
    async fn post(&self, url: &str) -> Result<Response> {
        self.request_with_retry("POST", url, |client, url| client.post(url).send())
            .await
    }

    /// Execute a DELETE request with retry logic
    async fn delete(&self, url: &str) -> Result<Response> {
        self.request_with_retry("DELETE", url, |client, url| client.delete(url).send())
            .await
    }

    /// Percent-encode a query string for safe URL usage
    fn encode_query(query: &str) -> String {
        query
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{:02X}", b),
            })
            .collect()
    }
}

impl GalleryEndpoints for NhClient {
    async fn get_galleries(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedResponse<GalleryListItem>> {
        let url = format!(
            "{}/api/v2/galleries?page={}&per_page={}",
            self.config.base_url, page, per_page
        );
        debug!("Fetching galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<GalleryListItem> = response.json().await?;
        Ok(result)
    }

    async fn get_galleries_tagged(
        &self,
        tag_id: u64,
        sort: Sort,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedResponse<GalleryListItem>> {
        let url = format!(
            "{}/api/v2/galleries/tagged?tag_id={}&sort={}&page={}&per_page={}",
            self.config.base_url,
            tag_id,
            sort.as_str(),
            page,
            per_page
        );
        debug!("Fetching tagged galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<GalleryListItem> = response.json().await?;
        Ok(result)
    }

    async fn get_popular_galleries(&self) -> Result<Vec<GalleryListItem>> {
        let url = format!("{}/api/v2/galleries/popular", self.config.base_url);
        debug!("Fetching popular galleries: {}", url);

        let response = self.get(&url).await?;
        let result: Vec<GalleryListItem> = response.json().await?;
        Ok(result)
    }

    async fn get_random_gallery(&self) -> Result<u64> {
        let url = format!("{}/api/v2/galleries/random", self.config.base_url);
        debug!("Fetching random gallery: {}", url);

        let response = self.get(&url).await?;
        let result: serde_json::Value = response.json().await?;
        result
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                Error::Json(
                    serde_json::from_str::<serde_json::Value>("")
                        .expect_err("empty string is invalid JSON"),
                )
            })
    }

    async fn get_gallery(&self, id: u64, include: Option<&str>) -> Result<GalleryDetailResponse> {
        let mut url = format!("{}/api/v2/galleries/{}", self.config.base_url, id);
        if let Some(inc) = include {
            url.push_str(&format!("?include={}", Self::encode_query(inc)));
        }
        debug!("Fetching gallery: {}", url);

        let response = self.get(&url).await?;
        let gallery: GalleryDetailResponse = response.json().await?;
        Ok(gallery)
    }

    async fn get_related_galleries(&self, id: u64) -> Result<RelatedGalleriesResponse> {
        let url = format!("{}/api/v2/galleries/{}/related", self.config.base_url, id);
        debug!("Fetching related galleries: {}", url);

        let response = self.get(&url).await?;
        let result: RelatedGalleriesResponse = response.json().await?;
        Ok(result)
    }

    async fn search(
        &self,
        query: &str,
        sort: Sort,
        page: u32,
    ) -> Result<PaginatedResponse<GalleryListItem>> {
        let encoded = Self::encode_query(query);
        let url = format!(
            "{}/api/v2/search?query={}&sort={}&page={}",
            self.config.base_url,
            encoded,
            sort.as_str(),
            page
        );
        debug!("Searching galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<GalleryListItem> = response.json().await?;
        Ok(result)
    }

    async fn download_gallery(&self, id: u64, format: DownloadFormat) -> Result<DownloadResponse> {
        let url = format!(
            "{}/api/v2/galleries/{}/download?format={}",
            self.config.base_url,
            id,
            format.as_str()
        );
        debug!("Requesting download URL: {}", url);

        let response = self.post(&url).await?;
        let result: DownloadResponse = response.json().await?;
        Ok(result)
    }

    async fn get_cdn_config(&self) -> Result<CdnConfig> {
        Self::fetch_cdn_config_inner(&self.client, &self.config).await
    }

    async fn get_tags_by_ids(&self, ids: &[u64]) -> Result<Vec<TagResponse>> {
        let ids_str: Vec<String> = ids.iter().map(ToString::to_string).collect();
        let url = format!(
            "{}/api/v2/tags/ids?ids={}",
            self.config.base_url,
            ids_str.join(",")
        );
        debug!("Fetching tags by IDs: {}", url);

        let response = self.get(&url).await?;
        let result: Vec<TagResponse> = response.json().await?;
        Ok(result)
    }

    async fn get_tags_by_type(
        &self,
        tag_type: &str,
        sort: TagSort,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedResponse<TagResponse>> {
        let url = format!(
            "{}/api/v2/tags/{}?sort={}&page={}&per_page={}",
            self.config.base_url,
            tag_type,
            sort.as_str(),
            page,
            per_page
        );
        debug!("Fetching tags by type: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<TagResponse> = response.json().await?;
        Ok(result)
    }

    async fn get_favorites(&self, page: u32) -> Result<PaginatedResponse<GalleryListItem>> {
        let url = format!("{}/api/v2/favorites?page={}", self.config.base_url, page);
        debug!("Fetching favorites: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<GalleryListItem> = response.json().await?;
        Ok(result)
    }

    async fn add_favorite(&self, gallery_id: u64) -> Result<FavoriteResponse> {
        let url = format!(
            "{}/api/v2/galleries/{}/favorite",
            self.config.base_url, gallery_id
        );
        debug!("Adding favorite: {}", url);

        let response = self.post(&url).await?;
        let result: FavoriteResponse = response.json().await?;
        Ok(result)
    }

    async fn remove_favorite(&self, gallery_id: u64) -> Result<FavoriteResponse> {
        let url = format!(
            "{}/api/v2/galleries/{}/favorite",
            self.config.base_url, gallery_id
        );
        debug!("Removing favorite: {}", url);

        let response = self.delete(&url).await?;
        let result: FavoriteResponse = response.json().await?;
        Ok(result)
    }
}
