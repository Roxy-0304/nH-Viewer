use serde::{Deserialize, Serialize};

/// CDN configuration response from /api/v2/cdn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnConfig {
    /// Available image server URLs
    pub image_servers: Vec<String>,
    /// Available thumbnail server URLs
    pub thumb_servers: Vec<String>,
}

impl Default for CdnConfig {
    fn default() -> Self {
        Self {
            image_servers: vec![
                "https://i.nhentai.net".to_string(),
                "https://i1.nhentai.net".to_string(),
                "https://i2.nhentai.net".to_string(),
            ],
            thumb_servers: vec![
                "https://t.nhentai.net".to_string(),
                "https://t1.nhentai.net".to_string(),
                "https://t2.nhentai.net".to_string(),
            ],
        }
    }
}

impl CdnConfig {
    /// Get an image server URL by index, wrapping around if out of bounds
    pub fn image_server(&self, index: usize) -> &str {
        if self.image_servers.is_empty() {
            return "https://i.nhentai.net";
        }
        &self.image_servers[index % self.image_servers.len()]
    }

    /// Get a thumbnail server URL by index, wrapping around if out of bounds
    pub fn thumb_server(&self, index: usize) -> &str {
        if self.thumb_servers.is_empty() {
            return "https://t.nhentai.net";
        }
        &self.thumb_servers[index % self.thumb_servers.len()]
    }
}
