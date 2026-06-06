use serde::{Deserialize, Serialize};

/// Paginated response from nhentai API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub result: Vec<T>,
    pub num_pages: u32,
    pub per_page: u32,
}
