use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use tracing::{debug, warn};

use crate::endpoints::GalleryEndpoints;
use crate::error::{Error, Result};
use crate::types::{CdnConfig, Gallery, PaginatedResponse};

/// Full browser-like User-Agent string (Chrome on Windows)
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// Configuration for the nhentai API client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL for the API
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
    /// Optional HTTP/SOCKS5 proxy URL (e.g. "http://127.0.0.1:7897")
    pub proxy: Option<String>,
    /// Whether to enable cookie jar for session persistence
    pub cookie_store: bool,
    /// Path to cookie file for persistence across restarts (requires cookie_store = true)
    pub cookie_file: Option<std::path::PathBuf>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://nhentai.net".to_string(),
            api_key: Some("nhk_3ctklttHlQf3la3PlyYH_Z95aWzt1xHZvFDrye_PYJ1kCW8q".to_string()),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            enable_dynamic_cdn: true,
            proxy: None,
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
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the base delay for exponential backoff
    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }

    /// Enable or disable dynamic CDN configuration
    pub fn dynamic_cdn(mut self, enable: bool) -> Self {
        self.enable_dynamic_cdn = enable;
        self
    }

    /// Set an HTTP/SOCKS5 proxy
    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// Enable or disable cookie jar
    pub fn cookies(mut self, enable: bool) -> Self {
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
    /// Create a new client with the given configuration.
    ///
    /// If `enable_dynamic_cdn` is true in the config, this will attempt to
    /// fetch CDN configuration from `/api/v2/cdn`. If the fetch fails,
    /// a default fallback configuration is used.
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .timeout(config.timeout);

        // Proxy
        if let Some(ref proxy_url) = config.proxy {
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|e| Error::CdnConfigFetch { reason: format!("invalid proxy: {e}") })?;
            builder = builder.proxy(proxy);
        }

        // Cookie jar
        if config.cookie_store {
            builder = builder.cookie_provider(Arc::new(reqwest::cookie::Jar::default()));
        }

        // Browser-like headers — keep it natural, avoid triggering bot detection
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
        // Note: Accept-Encoding is handled automatically by reqwest's gzip/brotli features
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

        let client = builder
            .default_headers(headers)
            .build()?;

        let cdn_config = if config.enable_dynamic_cdn {
            match Self::fetch_cdn_config_inner(&client, &config).await {
                Ok(cdn) => {
                    debug!("CDN config fetched successfully: {} image servers, {} thumb servers",
                        cdn.image_servers.len(), cdn.thumb_servers.len());
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
    pub fn new_static(config: ClientConfig) -> Self {
        let mut builder = Client::builder().timeout(config.timeout);

        if let Some(ref proxy_url) = config.proxy {
            let proxy = reqwest::Proxy::all(proxy_url)
                .expect("Invalid proxy URL");
            builder = builder.proxy(proxy);
        }

        if config.cookie_store {
            builder = builder.cookie_provider(Arc::new(reqwest::cookie::Jar::default()));
        }

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
                    .expect("Invalid API key header value"),
            );
        }

        let client = builder
            .default_headers(headers)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            config,
            cdn_config: Arc::new(CdnConfig::default()),
        }
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

    /// Execute a GET request with retry logic
    async fn get(&self, url: &str) -> Result<Response> {
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.retry_delay * 2u32.pow(attempt - 1);
                debug!("Retry attempt {} after {:?}", attempt, delay);
                tokio::time::sleep(delay).await;
            }

            match self.client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return Ok(response);
                    }

                    // Handle specific error codes
                    match status {
                        StatusCode::TOO_MANY_REQUESTS => {
                            let retry_after = response
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.parse::<u64>().ok())
                                .map(Duration::from_secs);

                            warn!("Rate limited, retry after {:?}", retry_after);

                            if attempt < self.config.max_retries {
                                let delay = retry_after.unwrap_or(
                                    self.config.retry_delay * 2u32.pow(attempt),
                                );
                                tokio::time::sleep(delay).await;
                                continue;
                            }

                            return Err(Error::RateLimited { retry_after });
                        }
                        StatusCode::NOT_FOUND => {
                            return Err(Error::HttpError {
                                status: status.as_u16(),
                                body: "Not found".to_string(),
                            });
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
                    warn!("Request failed: {}", e);
                    // If this is the last attempt, return the error
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
}

impl GalleryEndpoints for NhClient {
    async fn get_gallery(&self, id: u64) -> Result<Gallery> {
        let url = format!("{}/api/gallery/{}", self.config.base_url, id);
        debug!("Fetching gallery: {}", url);

        let response = self.get(&url).await?;
        let gallery: Gallery = response.json().await?;
        Ok(gallery)
    }

    async fn search(&self, query: &str, page: u32) -> Result<PaginatedResponse<Gallery>> {
        // Percent-encode the query for safe URL usage
        let encoded: String = query.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{:02X}", b),
            })
            .collect();
        let url = format!(
            "{}/api/galleries/search?query={}&page={}",
            self.config.base_url, encoded, page
        );
        debug!("Searching galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<Gallery> = response.json().await?;
        Ok(result)
    }

    async fn tagged(&self, tag_id: u64, page: u32) -> Result<PaginatedResponse<Gallery>> {
        let url = format!(
            "{}/api/galleries/tagged?tag_id={}&page={}",
            self.config.base_url, tag_id, page
        );
        debug!("Fetching tagged galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<Gallery> = response.json().await?;
        Ok(result)
    }

    async fn all(&self, page: u32) -> Result<PaginatedResponse<Gallery>> {
        let url = format!("{}/api/galleries/all?page={}", self.config.base_url, page);
        debug!("Fetching all galleries: {}", url);

        let response = self.get(&url).await?;
        let result: PaginatedResponse<Gallery> = response.json().await?;
        Ok(result)
    }
}