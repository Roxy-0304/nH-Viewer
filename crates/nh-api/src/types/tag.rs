use serde::{Deserialize, Serialize};

/// Type of tag in nhentai
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagType {
    Artist,
    Character,
    Group,
    Language,
    Parody,
    Category,
    Tag,
}

impl std::fmt::Display for TagType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagType::Artist => write!(f, "artist"),
            TagType::Character => write!(f, "character"),
            TagType::Group => write!(f, "group"),
            TagType::Language => write!(f, "language"),
            TagType::Parody => write!(f, "parody"),
            TagType::Category => write!(f, "category"),
            TagType::Tag => write!(f, "tag"),
        }
    }
}

/// A tag associated with a gallery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub tag_type: TagType,
    pub url: String,
    pub count: u64,
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}