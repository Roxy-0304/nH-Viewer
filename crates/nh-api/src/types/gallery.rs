use serde::{Deserialize, Serialize};

use super::tag::TagResponse;
use super::user::UserPublic;

// ---------------------------------------------------------------------------
// Gallery Title
// ---------------------------------------------------------------------------

/// Gallery title in multiple languages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryTitle {
    pub english: String,
    #[serde(default)]
    pub japanese: Option<String>,
    pub pretty: String,
}

impl GalleryTitle {
    /// Return the best available title (prefer pretty, fallback to english).
    pub fn best(&self) -> &str {
        if !self.pretty.is_empty() {
            &self.pretty
        } else {
            &self.english
        }
    }
}

// ---------------------------------------------------------------------------
// Cover / Thumbnail Info
// ---------------------------------------------------------------------------

/// Cover or thumbnail image with path and dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverInfo {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Page Info
// ---------------------------------------------------------------------------

/// Full page/image details for reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub number: u32,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub thumbnail: String,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
}

// ---------------------------------------------------------------------------
// Gallery List Item (lightweight for list views)
// ---------------------------------------------------------------------------

/// Lightweight gallery for list views.
/// Used in search results, tag listings, homepage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryListItem {
    pub id: u64,
    pub media_id: String,
    pub english_title: String,
    #[serde(default)]
    pub japanese_title: Option<String>,
    pub thumbnail: String,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    #[serde(default)]
    pub num_pages: u32,
    #[serde(default)]
    pub num_favorites: u32,
    #[serde(default)]
    pub tag_ids: Vec<u64>,
    #[serde(default)]
    pub blacklisted: bool,
}

impl GalleryListItem {
    /// Return the best available title.
    ///
    /// Prefers `english_title` when non-empty, falls back to
    /// `japanese_title` if present.
    pub fn best_title(&self) -> &str {
        if !self.english_title.is_empty() {
            &self.english_title
        } else if let Some(ref jp) = self.japanese_title {
            jp
        } else {
            &self.english_title
        }
    }
}

// ---------------------------------------------------------------------------
// Gallery Detail Response (full detail with optional includes)
// ---------------------------------------------------------------------------

/// Gallery detail with optional included data (comments, related, favorite, suggestions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryDetailResponse {
    pub id: u64,
    pub media_id: String,
    pub title: GalleryTitle,
    pub cover: CoverInfo,
    pub thumbnail: CoverInfo,
    #[serde(default)]
    pub scanlator: String,
    pub upload_date: u64,
    #[serde(default)]
    pub tags: Vec<TagResponse>,
    pub num_pages: u32,
    pub num_favorites: u32,
    #[serde(default)]
    pub pages: Vec<PageInfo>,
    #[serde(default)]
    pub comments: Option<Vec<CommentResponse>>,
    #[serde(default)]
    pub comment_count: Option<u32>,
    #[serde(default)]
    pub related: Option<Vec<GalleryListItem>>,
    #[serde(default)]
    pub is_favorited: Option<bool>,
    #[serde(default)]
    pub suggestions: Option<GallerySuggestionsBundle>,
}

// ---------------------------------------------------------------------------
// Comment
// ---------------------------------------------------------------------------

/// Comment response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentResponse {
    pub id: u64,
    pub gallery_id: u64,
    pub poster: UserPublic,
    pub post_date: u64,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Gallery Suggestions Bundle
// ---------------------------------------------------------------------------

/// Gallery-detail include payload for suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GallerySuggestionsBundle {
    #[serde(default)]
    pub trending: Vec<SuggestionResponse>,
    #[serde(default)]
    pub active: Vec<SuggestionResponse>,
    #[serde(default)]
    pub mine: Vec<SuggestionResponse>,
    pub counts: SuggestionTierCounts,
}

/// Suggestion tier counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionTierCounts {
    #[serde(default)]
    pub trending: u32,
    #[serde(default)]
    pub active: u32,
    #[serde(default)]
    pub declined: u32,
    #[serde(default)]
    pub hidden: u32,
}

/// Single suggestion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionResponse {
    pub id: String,
    pub gallery_id: u64,
    pub tag: SuggestionTag,
    pub action: String,
    pub status: String,
    #[serde(default)]
    pub score: Option<i32>,
    #[serde(default)]
    pub voter_count: u32,
    pub proposer: SuggestionProposer,
    pub created_at: String,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub resolver: Option<SuggestionProposer>,
    #[serde(default)]
    pub resolution_note: Option<String>,
    #[serde(default)]
    pub reverted_at: Option<String>,
    #[serde(default)]
    pub reverter: Option<SuggestionProposer>,
    #[serde(default)]
    pub my_vote: Option<i32>,
    #[serde(default)]
    pub tier: Option<String>,
}

/// Tag info in a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionTag {
    pub id: u64,
    #[serde(rename = "type")]
    pub tag_type: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Proposer info in a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionProposer {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Favorite Response
// ---------------------------------------------------------------------------

/// Response for favorite/unfavorite actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteResponse {
    pub favorited: bool,
    #[serde(default)]
    pub num_favorites: Option<u64>,
}

// ---------------------------------------------------------------------------
// Download Response
// ---------------------------------------------------------------------------

/// Download URL response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResponse {
    pub url: String,
    pub expires_at: u64,
}

// ---------------------------------------------------------------------------
// Related Galleries Response
// ---------------------------------------------------------------------------

/// Response for related galleries endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedGalleriesResponse {
    #[serde(default)]
    pub result: Vec<GalleryListItem>,
}

// ---------------------------------------------------------------------------
// Image File Type (kept for CDN URL construction)
// ---------------------------------------------------------------------------

/// Image file type for CDN URL construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFileType {
    Jpg,
    Png,
    Gif,
}

impl ImageFileType {
    /// Convert to the file extension string.
    pub const fn as_extension(&self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
        }
    }
}

impl std::fmt::Display for ImageFileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_extension())
    }
}
