use crate::error::Result;
use crate::types::{
    DownloadResponse, FavoriteResponse, GalleryDetailResponse, GalleryListItem, PaginatedResponse,
    RelatedGalleriesResponse, TagResponse,
};

/// Sort order for gallery listings and search.
#[derive(Debug, Clone, Copy, Default)]
pub enum Sort {
    #[default]
    Date,
    Popular,
    PopularToday,
    PopularWeek,
    PopularMonth,
}

impl Sort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sort::Date => "date",
            Sort::Popular => "popular",
            Sort::PopularToday => "popular-today",
            Sort::PopularWeek => "popular-week",
            Sort::PopularMonth => "popular-month",
        }
    }
}

/// Tag sort order.
#[derive(Debug, Clone, Copy, Default)]
pub enum TagSort {
    Name,
    #[default]
    Popular,
}

impl TagSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            TagSort::Name => "name",
            TagSort::Popular => "popular",
        }
    }
}

/// Download format.
#[derive(Debug, Clone, Copy, Default)]
pub enum DownloadFormat {
    #[default]
    Zip,
    Cbz,
    Torrent,
}

impl DownloadFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            DownloadFormat::Zip => "zip",
            DownloadFormat::Cbz => "cbz",
            DownloadFormat::Torrent => "torrent",
        }
    }
}

/// Trait providing high-level API endpoint methods matching the v2 spec.
pub trait GalleryEndpoints {
    /// Get paginated galleries ordered by newest first.
    /// `GET /api/v2/galleries`
    fn get_galleries(
        &self,
        page: u32,
        per_page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<GalleryListItem>>> + Send;

    /// Get galleries with a specific tag.
    /// `GET /api/v2/galleries/tagged`
    fn get_galleries_tagged(
        &self,
        tag_id: u64,
        sort: Sort,
        page: u32,
        per_page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<GalleryListItem>>> + Send;

    /// Get today's popular galleries.
    /// `GET /api/v2/galleries/popular`
    fn get_popular_galleries(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<GalleryListItem>>> + Send;

    /// Get a random gallery ID.
    /// `GET /api/v2/galleries/random`
    fn get_random_gallery(&self) -> impl std::future::Future<Output = Result<u64>> + Send;

    /// Get a single gallery with full details and optional includes.
    /// `GET /api/v2/galleries/{gallery_id}`
    ///
    /// `include` is a comma-separated string: "comments,related,favorite,suggestions"
    fn get_gallery(
        &self,
        id: u64,
        include: Option<&str>,
    ) -> impl std::future::Future<Output = Result<GalleryDetailResponse>> + Send;

    /// Get galleries similar to the specified gallery.
    /// `GET /api/v2/galleries/{gallery_id}/related`
    fn get_related_galleries(
        &self,
        id: u64,
    ) -> impl std::future::Future<Output = Result<RelatedGalleriesResponse>> + Send;

    /// Search galleries.
    /// `GET /api/v2/search`
    fn search(
        &self,
        query: &str,
        sort: Sort,
        page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<GalleryListItem>>> + Send;

    /// Get a download URL for a gallery.
    /// `POST /api/v2/galleries/{gallery_id}/download`
    fn download_gallery(
        &self,
        id: u64,
        format: DownloadFormat,
    ) -> impl std::future::Future<Output = Result<DownloadResponse>> + Send;

    /// Get CDN configuration.
    /// `GET /api/v2/cdn`
    fn get_cdn_config(
        &self,
    ) -> impl std::future::Future<Output = Result<crate::types::CdnConfig>> + Send;

    // --- Tag endpoints ---

    /// Look up multiple tags by ID. Max 100 per request.
    /// `GET /api/v2/tags/ids`
    fn get_tags_by_ids(
        &self,
        ids: &[u64],
    ) -> impl std::future::Future<Output = Result<Vec<TagResponse>>> + Send;

    /// Get tags of a specific type with pagination.
    /// `GET /api/v2/tags/{tag_type}`
    fn get_tags_by_type(
        &self,
        tag_type: &str,
        sort: TagSort,
        page: u32,
        per_page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<TagResponse>>> + Send;

    // --- Favorite endpoints ---

    /// Get the authenticated user's favorite galleries.
    /// `GET /api/v2/favorites`
    fn get_favorites(
        &self,
        page: u32,
    ) -> impl std::future::Future<Output = Result<PaginatedResponse<GalleryListItem>>> + Send;

    /// Add a gallery to favorites.
    /// `POST /api/v2/galleries/{gallery_id}/favorite`
    fn add_favorite(
        &self,
        gallery_id: u64,
    ) -> impl std::future::Future<Output = Result<FavoriteResponse>> + Send;

    /// Remove a gallery from favorites.
    /// `DELETE /api/v2/galleries/{gallery_id}/favorite`
    fn remove_favorite(
        &self,
        gallery_id: u64,
    ) -> impl std::future::Future<Output = Result<FavoriteResponse>> + Send;
}
