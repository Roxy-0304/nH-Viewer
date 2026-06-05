use crate::error::{Error, Result};
use crate::types::{CdnConfig, Gallery};

// ---------------------------------------------------------------------------
// Low-level pure URL helpers (no CdnConfig dependency)
// ---------------------------------------------------------------------------

/// Generate an image URL given an explicit server base, media_id, page, and extension.
///
/// This is the most primitive builder – it does **not** consult `CdnConfig`.
pub fn image_url(server: &str, media_id: &str, page: u32, ext: &str) -> String {
    format!("{}/{}/{}.{}", server, media_id, page, ext)
}

/// Generate a thumbnail URL given an explicit server base, media_id, and page.
pub fn thumbnail_url(server: &str, media_id: &str, page: u32) -> String {
    format!("{}/{}/{}t.jpg", server, media_id, page)
}

/// Generate a cover URL given an explicit server base, media_id, and extension.
pub fn cover_url(server: &str, media_id: &str, ext: &str) -> String {
    format!("{}/{}/cover.{}", server, media_id, ext)
}

// ---------------------------------------------------------------------------
// Dynamic CDN helpers (operate on CdnConfig + server_index)
// ---------------------------------------------------------------------------

/// Build the full image URL for a page using the dynamic CDN configuration.
///
/// # Arguments
/// * `cdn`          – current CDN configuration (image / thumb server lists).
/// * `media_id`     – gallery media id (string form, e.g. `"123456"`).
/// * `page`         – 1-based page number.
/// * `ext`          – file extension, e.g. `"jpg"`, `"png"`.
/// * `server_index` – index into `cdn.image_servers`; wraps around if out of bounds.
pub fn get_image_url(
    cdn: &CdnConfig,
    media_id: &str,
    page: u32,
    ext: &str,
    server_index: usize,
) -> String {
    let server = cdn.image_server(server_index);
    image_url(server, media_id, page, ext)
}

/// Build the full thumbnail URL for a page using the dynamic CDN configuration.
///
/// # Arguments
/// * `cdn`          – current CDN configuration.
/// * `media_id`     – gallery media id.
/// * `page`         – 1-based page number.
/// * `server_index` – index into `cdn.thumb_servers`; wraps around if out of bounds.
pub fn get_thumbnail_url(
    cdn: &CdnConfig,
    media_id: &str,
    page: u32,
    server_index: usize,
) -> String {
    let server = cdn.thumb_server(server_index);
    thumbnail_url(server, media_id, page)
}

/// Build the cover thumbnail URL using the dynamic CDN configuration.
pub fn get_cover_url(
    cdn: &CdnConfig,
    media_id: &str,
    ext: &str,
    server_index: usize,
) -> String {
    let server = cdn.thumb_server(server_index);
    cover_url(server, media_id, ext)
}

// ---------------------------------------------------------------------------
// Gallery convenience wrappers
// ---------------------------------------------------------------------------

/// Get the image URL for a specific page of a gallery using dynamic CDN.
///
/// # Arguments
/// * `gallery`      – the gallery object.
/// * `page`         – 1-based page number.
/// * `server_index` – index into `cdn.image_servers`.
///
/// # Errors
/// Returns an error if `page` is out of bounds.
pub fn gallery_page_url(
    cdn: &CdnConfig,
    gallery: &Gallery,
    page: u32,
    server_index: usize,
) -> Result<String> {
    if page == 0 || page > gallery.images.pages.len() as u32 {
        return Err(Error::InvalidPageNumber {
            page,
            max: gallery.images.pages.len() as u32,
        });
    }

    let ext = gallery
        .images
        .pages
        .get((page - 1) as usize)
        .ok_or(Error::ImageExtensionNotFound { page })?;

    Ok(get_image_url(
        cdn,
        &gallery.media_id,
        page,
        ext.as_extension(),
        server_index,
    ))
}

/// Get the thumbnail URL for a specific page of a gallery using dynamic CDN.
pub fn gallery_page_thumbnail_url(
    cdn: &CdnConfig,
    gallery: &Gallery,
    page: u32,
    server_index: usize,
) -> Result<String> {
    if page == 0 || page > gallery.images.pages.len() as u32 {
        return Err(Error::InvalidPageNumber {
            page,
            max: gallery.images.pages.len() as u32,
        });
    }

    Ok(get_thumbnail_url(cdn, &gallery.media_id, page, server_index))
}

/// Get the cover URL for a gallery using dynamic CDN.
pub fn gallery_cover_url(cdn: &CdnConfig, gallery: &Gallery, server_index: usize) -> String {
    get_cover_url(
        cdn,
        &gallery.media_id,
        gallery.images.cover.as_extension(),
        server_index,
    )
}