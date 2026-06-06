pub mod client;
pub mod endpoints;
pub mod error;
pub mod image_url;
pub mod prelude;
pub mod types;

// Re-export key types for convenience
pub use client::{ClientConfig, NhClient, ProxyMode};
pub use endpoints::{DownloadFormat, GalleryEndpoints, Sort, TagSort};
pub use error::{Error, Result};
pub use types::{
    CdnConfig, CommentResponse, CoverInfo, DownloadResponse, FavoriteResponse,
    GalleryDetailResponse, GalleryListItem, GallerySuggestionsBundle, GalleryTitle,
    ImageFileType, PageInfo, PaginatedResponse, RelatedGalleriesResponse, TagResponse,
    UserPublic,
};