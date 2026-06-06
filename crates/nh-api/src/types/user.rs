use serde::{Deserialize, Serialize};

/// Public user information (shown in comments, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPublic {
    pub id: u64,
    pub username: String,
    pub slug: String,
    pub avatar_url: String,
    #[serde(default)]
    pub is_superuser: bool,
    #[serde(default)]
    pub is_staff: bool,
}
