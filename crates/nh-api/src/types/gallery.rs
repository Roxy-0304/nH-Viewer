use serde::{Deserialize, Serialize};

use super::tag::Tag;

/// Image file type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFileType {
    Jpg,
    Png,
    Gif,
}

impl ImageFileType {
    /// Convert to the file extension string
    pub fn as_extension(&self) -> &'static str {
        match self {
            ImageFileType::Jpg => "jpg",
            ImageFileType::Png => "png",
            ImageFileType::Gif => "gif",
        }
    }
}

impl std::fmt::Display for ImageFileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_extension())
    }
}

/// Images metadata for a gallery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Images {
    pub pages: Vec<ImageFileType>,
    pub cover: ImageFileType,
    pub thumbnail: ImageFileType,
}

/// Title of a gallery (English and Japanese)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Title {
    pub english: Option<String>,
    pub japanese: Option<String>,
    pub pretty: Option<String>,
}

impl Title {
    /// Get the best available title (preferred order: english -> japanese -> pretty)
    pub fn best(&self) -> &str {
        self.english
            .as_deref()
            .or(self.japanese.as_deref())
            .or(self.pretty.as_deref())
            .unwrap_or("Untitled")
    }
}

/// A single gallery/doujinshi
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gallery {
    pub id: u64,
    pub media_id: String,
    pub title: Title,
    pub images: Images,
    pub tags: Vec<Tag>,
    pub num_pages: u32,
    pub num_favorites: u32,
    pub scanlator: Option<String>,
    pub upload_date: u64,
}

impl Gallery {
    /// Get the best title
    pub fn best_title(&self) -> &str {
        self.title.best()
    }

    /// Get tags filtered by type
    pub fn tags_by_type(&self, tag_type: &super::tag::TagType) -> Vec<&Tag> {
        self.tags
            .iter()
            .filter(|t| t.tag_type == *tag_type)
            .collect()
    }

    /// Get language tags
    pub fn languages(&self) -> Vec<&Tag> {
        self.tags_by_type(&super::tag::TagType::Language)
    }

    /// Get artist tags
    pub fn artists(&self) -> Vec<&Tag> {
        self.tags_by_type(&super::tag::TagType::Artist)
    }
}