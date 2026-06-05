use rusqlite::{params, Connection};

use crate::error::{Error, Result};

/// A single favorite entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FavoriteEntry {
    pub id: i64,
    pub gallery_id: i64,
    pub title: String,
    pub cover_path: Option<String>,
    /// Unix timestamp (seconds)
    pub added_at: i64,
}

/// Add a gallery to favorites. Returns the new entry id.
///
/// If the gallery is already in favorites, returns `Error::Duplicate`.
pub fn add(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
    cover_path: Option<&str>,
) -> Result<i64> {
    // Check for existing entry
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM favorites WHERE gallery_id = ?1",
            params![gallery_id],
            |row| {
                let count: i64 = row.get(0)?;
                Ok(count > 0)
            },
        )
        .unwrap_or(false);

    if exists {
        return Err(Error::Duplicate {
            entity: "favorite".to_string(),
            key: gallery_id.to_string(),
        });
    }

    conn.execute(
        "INSERT INTO favorites (gallery_id, title, cover_path) VALUES (?1, ?2, ?3)",
        params![gallery_id, title, cover_path],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Remove a gallery from favorites. Returns true if an entry was deleted.
pub fn remove(conn: &Connection, gallery_id: i64) -> Result<bool> {
    let count = conn.execute(
        "DELETE FROM favorites WHERE gallery_id = ?1",
        params![gallery_id],
    )?;
    Ok(count > 0)
}

/// List all favorites, most recently added first
pub fn list(conn: &Connection, limit: u32, offset: u32) -> Result<Vec<FavoriteEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, gallery_id, title, cover_path, added_at
         FROM favorites ORDER BY added_at DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(FavoriteEntry {
            id: row.get(0)?,
            gallery_id: row.get(1)?,
            title: row.get(2)?,
            cover_path: row.get(3)?,
            added_at: row.get(4)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Check if a gallery is in favorites
pub fn is_favorite(conn: &Connection, gallery_id: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM favorites WHERE gallery_id = ?1",
        params![gallery_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Get a favorite entry by gallery id
pub fn by_gallery_id(conn: &Connection, gallery_id: i64) -> Result<FavoriteEntry> {
    conn.query_row(
        "SELECT id, gallery_id, title, cover_path, added_at
         FROM favorites WHERE gallery_id = ?1",
        params![gallery_id],
        |row| {
            Ok(FavoriteEntry {
                id: row.get(0)?,
                gallery_id: row.get(1)?,
                title: row.get(2)?,
                cover_path: row.get(3)?,
                added_at: row.get(4)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound {
            entity: "favorite".to_string(),
            key: gallery_id.to_string(),
        },
        other => Error::Sqlite(other),
    })
}

/// Get total favorites count
pub fn count(conn: &Connection) -> Result<u64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))?;
    Ok(count as u64)
}