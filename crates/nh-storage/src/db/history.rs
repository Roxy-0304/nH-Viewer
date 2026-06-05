use rusqlite::{params, Connection};

use crate::error::Result;

/// A single browsing history entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub gallery_id: i64,
    pub title: String,
    pub cover_path: Option<String>,
    /// Unix timestamp (seconds)
    pub viewed_at: i64,
    /// Last viewed page (1-based)
    pub page: u32,
}

/// Insert a new history entry
pub fn insert(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
    cover_path: Option<&str>,
    page: u32,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO history (gallery_id, title, cover_path, page) VALUES (?1, ?2, ?3, ?4)",
        params![gallery_id, title, cover_path, page],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get history entries, most recent first
pub fn list(conn: &Connection, limit: u32, offset: u32) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, gallery_id, title, cover_path, viewed_at, page
         FROM history ORDER BY viewed_at DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            gallery_id: row.get(1)?,
            title: row.get(2)?,
            cover_path: row.get(3)?,
            viewed_at: row.get(4)?,
            page: row.get(5)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Get history entries for a specific gallery
pub fn by_gallery_id(conn: &Connection, gallery_id: i64) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, gallery_id, title, cover_path, viewed_at, page
         FROM history WHERE gallery_id = ?1 ORDER BY viewed_at DESC",
    )?;
    let rows = stmt.query_map(params![gallery_id], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            gallery_id: row.get(1)?,
            title: row.get(2)?,
            cover_path: row.get(3)?,
            viewed_at: row.get(4)?,
            page: row.get(5)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Delete a specific history entry by id
pub fn delete(conn: &Connection, id: i64) -> Result<bool> {
    let count = conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(count > 0)
}

/// Clear all history
pub fn clear(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM history", [])?;
    Ok(count)
}

/// Get total history count
pub fn count(conn: &Connection) -> Result<u64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
    Ok(count as u64)
}