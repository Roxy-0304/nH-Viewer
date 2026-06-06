use serde::{Deserialize, Serialize};

/// Tag response matching the API spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagResponse {
    pub id: u64,
    #[serde(rename = "type")]
    pub tag_type: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub count: u64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_community: Option<bool>,
    #[serde(default)]
    pub pending_describe_id: Option<String>,
}