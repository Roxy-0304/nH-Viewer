//! # nh-api
//!
//! A Rust client library for the nhentai API.
//!
//! This crate provides a type-safe, async interface to interact with the nhentai API,
//! including gallery retrieval, search, and image URL generation.
//!
//! ## Features
//!
//! - Type-safe data models for galleries, tags, and pagination
//! - Configurable HTTP client with retry logic
//! - Image URL generation for thumbnails and full-size images
//! - Error handling for network, HTTP, and parsing errors
//!
//! ## Example
//!
//! ```rust,no_run
//! use nh_api::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = NhClient::with_api_key(None).await?;
//!     let gallery = client.get_gallery(12345).await?;
//!     println!("Title: {}", gallery.best_title());
//!
//!     // Use dynamic CDN to build image URLs
//!     let cdn = client.cdn_config();
//!     let url = get_image_url(cdn, &gallery.media_id, 1, "jpg", 0);
//!     println!("Page 1: {url}");
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod endpoints;
pub mod error;
pub mod image_url;
pub mod prelude;
pub mod types;

// Re-export main types for convenience
pub use client::{ClientConfig, NhClient};
pub use endpoints::GalleryEndpoints;
pub use error::{Error, Result};
pub use types::{CdnConfig, Gallery, ImageFileType, Images, PaginatedResponse, Tag, TagType, Title};
