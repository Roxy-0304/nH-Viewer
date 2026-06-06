use sqlx::SqlitePool;

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
pub async fn insert(
    pool: &SqlitePool,
    gallery_id: i64,
    title: &str,
    cover_path: Option<&str>,
    page: u32,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO history (gallery_id, title, cover_path, page) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(gallery_id)
    .bind(title)
    .bind(cover_path)
    .bind(page as i64)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Get history entries, most recent first
pub async fn list(pool: &SqlitePool, limit: u32, offset: u32) -> Result<Vec<HistoryEntry>> {
    let rows = sqlx::query_as::<_, (i64, i64, String, Option<String>, i64, i64)>(
        "SELECT id, gallery_id, title, cover_path, viewed_at, page
         FROM history ORDER BY viewed_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            id: r.0,
            gallery_id: r.1,
            title: r.2,
            cover_path: r.3,
            viewed_at: r.4,
            page: r.5 as u32,
        })
        .collect())
}

/// Get history entries for a specific gallery
pub async fn by_gallery_id(pool: &SqlitePool, gallery_id: i64) -> Result<Vec<HistoryEntry>> {
    let rows = sqlx::query_as::<_, (i64, i64, String, Option<String>, i64, i64)>(
        "SELECT id, gallery_id, title, cover_path, viewed_at, page
         FROM history WHERE gallery_id = ?1 ORDER BY viewed_at DESC",
    )
    .bind(gallery_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| HistoryEntry {
            id: r.0,
            gallery_id: r.1,
            title: r.2,
            cover_path: r.3,
            viewed_at: r.4,
            page: r.5 as u32,
        })
        .collect())
}

/// Delete a specific history entry by id
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM history WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Clear all history
pub async fn clear(pool: &SqlitePool) -> Result<usize> {
    let result = sqlx::query("DELETE FROM history").execute(pool).await?;
    Ok(result.rows_affected() as usize)
}

/// Get total history count
pub async fn count(pool: &SqlitePool) -> Result<u64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM history")
        .fetch_one(pool)
        .await?;
    Ok(count as u64)
}
