use rusqlite::{params, Connection};

use crate::error::Result;

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
// Gallery cache CRUD
// ---------------------------------------------------------------------------

/// Insert or replace a cached gallery.
pub fn upsert_gallery(conn: &Connection, g: &CachedGallery) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO galleries
         (id, media_id, title_en, title_jp, title_pretty, num_pages, num_favorites,
          cover_ext, tags_json, raw_json, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s', 'now'))",
        params![
            g.id as i64,
            g.media_id,
            g.title_en,
            g.title_jp,
            g.title_pretty,
            g.num_pages as i64,
            g.num_favorites as i64,
            g.cover_ext,
            g.tags_json,
            g.raw_json,
        ],
    )?;
    Ok(())
}

/// Get a cached gallery by id.
pub fn get_gallery(conn: &Connection, id: u64) -> Result<Option<CachedGallery>> {
    let result = conn.query_row(
        "SELECT id, media_id, title_en, title_jp, title_pretty,
                num_pages, num_favorites, cover_ext, tags_json, raw_json, cached_at
         FROM galleries WHERE id = ?1",
        params![id as i64],
        |row| {
            Ok(CachedGallery {
                id: row.get::<_, i64>(0)? as u64,
                media_id: row.get(1)?,
                title_en: row.get(2)?,
                title_jp: row.get(3)?,
                title_pretty: row.get(4)?,
                num_pages: row.get::<_, i64>(5)? as u32,
                num_favorites: row.get::<_, i64>(6)? as u32,
                cover_ext: row.get(7)?,
                tags_json: row.get(8)?,
                raw_json: row.get(9)?,
                cached_at: row.get(10)?,
            })
        },
    );
    match result {
        Ok(g) => Ok(Some(g)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List cached gallery previews, most recently cached first.
pub fn list_galleries(conn: &Connection, limit: u32, offset: u32) -> Result<Vec<GalleryPreview>> {
    let mut stmt = conn.prepare(
        "SELECT g.id,
                COALESCE(g.title_en, g.title_jp, g.title_pretty, 'Untitled') as title,
                g.num_pages, g.cover_ext
         FROM galleries g
         ORDER BY g.cached_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
        Ok(GalleryPreview {
            id: row.get::<_, i64>(0)? as u64,
            title: row.get(1)?,
            num_pages: row.get::<_, i64>(2)? as u32,
            cover_ext: row.get(3)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Search cached galleries by title (simple LIKE search).
pub fn search_galleries(
    conn: &Connection,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<GalleryPreview>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT g.id,
                COALESCE(g.title_en, g.title_jp, g.title_pretty, 'Untitled') as title,
                g.num_pages, g.cover_ext
         FROM galleries g
         WHERE g.title_en LIKE ?1 OR g.title_jp LIKE ?1 OR g.title_pretty LIKE ?1
         ORDER BY g.cached_at DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(
        params![pattern, limit as i64, offset as i64],
        |row| {
            Ok(GalleryPreview {
                id: row.get::<_, i64>(0)? as u64,
                title: row.get(1)?,
                num_pages: row.get::<_, i64>(2)? as u32,
                cover_ext: row.get(3)?,
            })
        },
    )?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Delete a cached gallery by id.
pub fn delete_gallery(conn: &Connection, id: u64) -> Result<bool> {
    let count = conn.execute("DELETE FROM galleries WHERE id = ?1", params![id as i64])?;
    Ok(count > 0)
}

// ---------------------------------------------------------------------------
// Search history CRUD
// ---------------------------------------------------------------------------

/// Record a search query.
pub fn record_search(conn: &Connection, query: &str, result_count: u32) -> Result<i64> {
    conn.execute(
        "INSERT INTO search_history (query, result_count) VALUES (?1, ?2)",
        params![query, result_count as i64],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List recent search history.
pub fn list_search_history(conn: &Connection, limit: u32) -> Result<Vec<SearchHistoryItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, query, result_count, searched_at
         FROM search_history
         ORDER BY searched_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(SearchHistoryItem {
            id: row.get(0)?,
            query: row.get(1)?,
            result_count: row.get::<_, i64>(2)? as u32,
            searched_at: row.get(3)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Clear all search history.
pub fn clear_search_history(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM search_history", [])?;
    Ok(count)
}