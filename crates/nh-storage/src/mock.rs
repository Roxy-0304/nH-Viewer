//! `MockGalleryRepository` — in-memory mock data source for Flutter UI prototyping.

use async_trait::async_trait;

use crate::db::gallery_cache::{CachedGallery, GalleryPreview, SearchHistoryItem};
use crate::repository::{GalleryRepository, RepositoryError};

/// In-memory gallery repository with hardcoded sample data.
///
/// Use this to prototype and test the Flutter UI without a network connection.
pub struct MockGalleryRepository {
    galleries: Vec<CachedGallery>,
}

impl MockGalleryRepository {
    /// Create a new mock repository with sample data covering common scenarios.
    pub fn new() -> Self {
        let galleries = vec![
            // 1. Popular English work — many tags, high favorites
            CachedGallery {
                id: 177013,
                media_id: "987564".to_string(),
                title_en: Some("Metamorphosis".to_string()),
                title_jp: Some("変身".to_string()),
                title_pretty: Some("Metamorphosis".to_string()),
                num_pages: 226,
                num_favorites: 12500,
                cover_ext: Some("jpg".to_string()),
                tags_json: Some(
                    r#"[{"id":1,"name":"doujinshi","type":"category","url":"/category/doujinshi/","count":0},
{"id":3,"name":"big breasts","type":"tag","url":"/tag/big-breasts/","count":0},
{"id":10,"name":"sole female","type":"tag","url":"/tag/sole-female/","count":0}]"#
                        .to_string(),
                ),
                raw_json: None,
                cached_at: 1700000000,
            },
            // 2. Japanese-only title — no English title
            CachedGallery {
                id: 286912,
                media_id: "1593210".to_string(),
                title_en: None,
                title_jp: Some("夏の日、君と泳ぐ。 第1-5話".to_string()),
                title_pretty: Some("夏の日、君と泳ぐ。".to_string()),
                num_pages: 32,
                num_favorites: 820,
                cover_ext: Some("jpg".to_string()),
                tags_json: Some(
                    r#"[{"id":2,"name":"manga","type":"category","url":"/category/manga/","count":0},
{"id":12,"name":"romance","type":"tag","url":"/tag/romance/","count":0}]"#
                        .to_string(),
                ),
                raw_json: None,
                cached_at: 1700100000,
            },
            // 3. Short work — few pages, few tags
            CachedGallery {
                id: 350180,
                media_id: "1890456".to_string(),
                title_en: Some("A Quick Sketch".to_string()),
                title_jp: Some("らくがき".to_string()),
                title_pretty: Some("A Quick Sketch".to_string()),
                num_pages: 4,
                num_favorites: 45,
                cover_ext: Some("png".to_string()),
                tags_json: Some(
                    r#"[{"id":1,"name":"doujinshi","type":"category","url":"/category/doujinshi/","count":0}]"#
                        .to_string(),
                ),
                raw_json: None,
                cached_at: 1700200000,
            },
            // 4. GIF work
            CachedGallery {
                id: 412056,
                media_id: "2200333".to_string(),
                title_en: Some("Animated Color Loop".to_string()),
                title_jp: None,
                title_pretty: Some("Animated Color Loop".to_string()),
                num_pages: 6,
                num_favorites: 310,
                cover_ext: Some("gif".to_string()),
                tags_json: Some(
                    r#"[{"id":4,"name":"artist cg","type":"category","url":"/category/artist-cg/","count":0},
{"id":11,"name":"animated","type":"tag","url":"/tag/animated/","count":0}]"#
                        .to_string(),
                ),
                raw_json: None,
                cached_at: 1700300000,
            },
            // 5. Group/party work — many pages
            CachedGallery {
                id: 500200,
                media_id: "2700100".to_string(),
                title_en: Some("The Great Festival Collection".to_string()),
                title_jp: Some("お祭り大全集".to_string()),
                title_pretty: Some("The Great Festival Collection".to_string()),
                num_pages: 58,
                num_favorites: 5700,
                cover_ext: Some("jpg".to_string()),
                tags_json: Some(
                    r#"[{"id":1,"name":"doujinshi","type":"category","url":"/category/doujinshi/","count":0},
{"id":3,"name":"big breasts","type":"tag","url":"/tag/big-breasts/","count":0},
{"id":12,"name":"romance","type":"tag","url":"/tag/romance/","count":0}]"#
                        .to_string(),
                ),
                raw_json: None,
                cached_at: 1700400000,
            },
        ];

        Self { galleries }
    }

    /// Alias for `new()`.
    pub fn with_sample_data() -> Self {
        Self::new()
    }

    /// Convert a CachedGallery to a lightweight GalleryPreview.
    fn to_preview(g: &CachedGallery) -> GalleryPreview {
        GalleryPreview {
            id: g.id,
            title: g.best_title().to_string(),
            num_pages: g.num_pages,
            cover_ext: g.cover_ext.clone(),
        }
    }
}

impl Default for MockGalleryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GalleryRepository for MockGalleryRepository {
    async fn get_gallery(&self, id: u64) -> Result<Option<CachedGallery>, RepositoryError> {
        Ok(self.galleries.iter().find(|g| g.id == id).cloned())
    }

    async fn search_galleries(
        &self,
        query: &str,
        _page: u32,
    ) -> Result<Vec<GalleryPreview>, RepositoryError> {
        let q = query.to_lowercase();
        Ok(self
            .galleries
            .iter()
            .filter(|g| {
                g.title_en
                    .as_deref()
                    .map(|t| t.to_lowercase().contains(&q))
                    .unwrap_or(false)
                    || g.title_jp
                        .as_deref()
                        .map(|t| t.contains(query))
                        .unwrap_or(false)
                    || g.title_pretty
                        .as_deref()
                        .map(|t| t.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .map(Self::to_preview)
            .collect())
    }

    async fn save_gallery(&self, _gallery: &CachedGallery) -> Result<(), RepositoryError> {
        // no-op in mock mode
        Ok(())
    }

    async fn record_search(
        &self,
        _query: &str,
        _result_count: u32,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }

    async fn list_search_history(
        &self,
        _limit: u32,
    ) -> Result<Vec<SearchHistoryItem>, RepositoryError> {
        Ok(vec![])
    }

    async fn delete_gallery(&self, _id: u64) -> Result<bool, RepositoryError> {
        Ok(true)
    }
}