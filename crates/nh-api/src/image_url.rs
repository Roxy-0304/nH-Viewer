use crate::types::CdnConfig;

/// Construct a full image URL from CDN config and a relative path.
///
/// The v2 API returns `path` fields (e.g. `/galleries/1234/1.jpg` or
/// `galleries/1234/1.jpg`) that need to be combined with a CDN server URL
/// from the CDN config.
///
/// `server_index` is used to distribute load across servers (typically the gallery ID mod server count).
pub fn build_image_url(cdn: &CdnConfig, path: &str, server_index: usize) -> String {
    let server = cdn.image_server(server_index);
    if path.starts_with('/') {
        format!("{}{}", server, path)
    } else {
        format!("{}/{}", server, path)
    }
}

/// Construct a full thumbnail URL from CDN config and a relative path.
pub fn build_thumb_url(cdn: &CdnConfig, path: &str, server_index: usize) -> String {
    let server = cdn.thumb_server(server_index);
    if path.starts_with('/') {
        format!("{}{}", server, path)
    } else {
        format!("{}/{}", server, path)
    }
}
