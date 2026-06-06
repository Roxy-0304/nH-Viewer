use sqlx::SqlitePool;

/// A cached gallery with full metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedGallery {
    pub id: u64,
    pub media_id: String,
    pub title_en: Option<String>,
    pub title_jp: Option<String>,
    pub title_pretty: Option<String>,
    pub num_pages: u32,
    pub num_favorites: u32,
    pub cover_ext: Option<String>,
    pub tags_json: Option<String>,
    pub raw_json: Option<String>,
    pub cached_at: i64,
}

impl CachedGallery {
    /// Get the best available title.
    pub fn best_title(&self) -> &str {
        self.title_en
            .as_deref()
            .or(self.title_jp.as_deref())
            .or(self.title_pretty.as_deref())
            .unwrap_or("Untitled")
    }
}

/// Lightweight gallery preview for list views.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GalleryPreview {
    pub id: u64,
    pub title: String,
    pub num_pages: u32,
    pub cover_ext: Option<String>,
}

/// A search history entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHistoryItem {
    pub id: i64,
    pub query: String,
    pub result_count: u32,
    pub searched_at: i64,
}

// ---------------------------------------------------------------------------
// Gallery cache CRUD (async via sqlx)
// ---------------------------------------------------------------------------

/// Insert or update a gallery's raw JSON payload.
///
/// Uses `ON CONFLICT(id) DO UPDATE` to only refresh the JSON and timestamp
/// when the gallery already exists.
pub async fn upsert_gallery(pool: &SqlitePool, id: u64, raw_json: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO galleries (id, raw_json, cached_at)
         VALUES (?1, ?2, strftime('%s', 'now'))
         ON CONFLICT(id) DO UPDATE SET
             raw_json = excluded.raw_json,
             cached_at = excluded.cached_at",
    )
    .bind(id as i64)
    .bind(raw_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert or replace a cached gallery with full metadata.
pub async fn upsert_gallery_full(pool: &SqlitePool, g: &CachedGallery) -> crate::error::Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO galleries
         (id, media_id, title_en, title_jp, title_pretty, num_pages, num_favorites,
          cover_ext, tags_json, raw_json, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s', 'now'))",
    )
    .bind(g.id as i64)
    .bind(&g.media_id)
    .bind(&g.title_en)
    .bind(&g.title_jp)
    .bind(&g.title_pretty)
    .bind(g.num_pages as i64)
    .bind(g.num_favorites as i64)
    .bind(&g.cover_ext)
    .bind(&g.tags_json)
    .bind(&g.raw_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a cached gallery's raw JSON string by id.
///
/// Returns `None` if no gallery with the given id is cached.
pub async fn get_gallery_raw(pool: &SqlitePool, id: u64) -> anyhow::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT raw_json FROM galleries WHERE id = ?1")
        .bind(id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|(raw_json,)| raw_json))
}

/// Get a cached gallery by id (full metadata).
pub async fn get_gallery(
    pool: &SqlitePool,
    id: u64,
) -> crate::error::Result<Option<CachedGallery>> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        ),
    >(
        "SELECT id, media_id, title_en, title_jp, title_pretty,
                num_pages, num_favorites, cover_ext, tags_json, raw_json, cached_at
         FROM galleries WHERE id = ?1",
    )
    .bind(id as i64)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(CachedGallery {
            id: r.0 as u64,
            media_id: r.1,
            title_en: r.2,
            title_jp: r.3,
            title_pretty: r.4,
            num_pages: r.5 as u32,
            num_favorites: r.6 as u32,
            cover_ext: r.7,
            tags_json: r.8,
            raw_json: r.9,
            cached_at: r.10,
        })),
        None => Ok(None),
    }
}

/// List cached gallery previews, most recently cached first.
pub async fn list_galleries(
    pool: &SqlitePool,
    limit: u32,
    offset: u32,
) -> crate::error::Result<Vec<GalleryPreview>> {
    let rows = sqlx::query_as::<_, (i64, String, i64, Option<String>)>(
        "SELECT g.id,
                COALESCE(g.title_en, g.title_jp, g.title_pretty, 'Untitled') as title,
                g.num_pages, g.cover_ext
         FROM galleries g
         ORDER BY g.cached_at DESC
         LIMIT ?1 OFFSET ?2",
    )
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| GalleryPreview {
            id: r.0 as u64,
            title: r.1,
            num_pages: r.2 as u32,
            cover_ext: r.3,
        })
        .collect())
}

/// Search cached galleries by title (simple LIKE search).
pub async fn search_galleries(
    pool: &SqlitePool,
    query: &str,
    limit: u32,
    offset: u32,
) -> crate::error::Result<Vec<GalleryPreview>> {
    let pattern = format!("%{}%", query);
    let rows = sqlx::query_as::<_, (i64, String, i64, Option<String>)>(
        "SELECT g.id,
                COALESCE(g.title_en, g.title_jp, g.title_pretty, 'Untitled') as title,
                g.num_pages, g.cover_ext
         FROM galleries g
         WHERE g.title_en LIKE ?1 OR g.title_jp LIKE ?1 OR g.title_pretty LIKE ?1
         ORDER BY g.cached_at DESC
         LIMIT ?2 OFFSET ?3",
    )
    .bind(&pattern)
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| GalleryPreview {
            id: r.0 as u64,
            title: r.1,
            num_pages: r.2 as u32,
            cover_ext: r.3,
        })
        .collect())
}

/// Delete a cached gallery by id.
pub async fn delete_gallery(pool: &SqlitePool, id: u64) -> crate::error::Result<bool> {
    let result = sqlx::query("DELETE FROM galleries WHERE id = ?1")
        .bind(id as i64)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// Search history CRUD (async via sqlx)
// ---------------------------------------------------------------------------

/// Record a search query.
pub async fn record_search(
    pool: &SqlitePool,
    query: &str,
    result_count: u32,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO search_history (query, result_count) VALUES (?1, ?2)")
        .bind(query)
        .bind(result_count as i64)
        .execute(pool)
        .await?;
    Ok(())
}

/// List recent search history.
pub async fn list_search_history(
    pool: &SqlitePool,
    limit: u32,
) -> crate::error::Result<Vec<SearchHistoryItem>> {
    let rows = sqlx::query_as::<_, (i64, String, i64, i64)>(
        "SELECT id, query, result_count, searched_at
         FROM search_history
         ORDER BY searched_at DESC
         LIMIT ?1",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SearchHistoryItem {
            id: r.0,
            query: r.1,
            result_count: r.2 as u32,
            searched_at: r.3,
        })
        .collect())
}

/// Clear all search history.
pub async fn clear_search_history(pool: &SqlitePool) -> crate::error::Result<usize> {
    let result = sqlx::query("DELETE FROM search_history")
        .execute(pool)
        .await?;
    Ok(result.rows_affected() as usize)
}
