use sqlx::SqlitePool;

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
pub async fn add(
    pool: &SqlitePool,
    gallery_id: i64,
    title: &str,
    cover_path: Option<&str>,
) -> Result<i64> {
    // Check for existing entry
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM favorites WHERE gallery_id = ?1",
    )
    .bind(gallery_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if count > 0 {
        return Err(Error::Duplicate {
            entity: "favorite".to_string(),
            key: gallery_id.to_string(),
        });
    }

    let result = sqlx::query(
        "INSERT INTO favorites (gallery_id, title, cover_path) VALUES (?1, ?2, ?3)",
    )
    .bind(gallery_id)
    .bind(title)
    .bind(cover_path)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Remove a gallery from favorites. Returns true if an entry was deleted.
pub async fn remove(pool: &SqlitePool, gallery_id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM favorites WHERE gallery_id = ?1")
        .bind(gallery_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List all favorites, most recently added first
pub async fn list(pool: &SqlitePool, limit: u32, offset: u32) -> Result<Vec<FavoriteEntry>> {
    let rows = sqlx::query_as::<_, (i64, i64, String, Option<String>, i64)>(
        "SELECT id, gallery_id, title, cover_path, added_at
         FROM favorites ORDER BY added_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| FavoriteEntry {
            id: r.0,
            gallery_id: r.1,
            title: r.2,
            cover_path: r.3,
            added_at: r.4,
        })
        .collect())
}

/// Check if a gallery is in favorites
pub async fn is_favorite(pool: &SqlitePool, gallery_id: i64) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM favorites WHERE gallery_id = ?1",
    )
    .bind(gallery_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Get a favorite entry by gallery id
pub async fn by_gallery_id(pool: &SqlitePool, gallery_id: i64) -> Result<FavoriteEntry> {
    let row = sqlx::query_as::<_, (i64, i64, String, Option<String>, i64)>(
        "SELECT id, gallery_id, title, cover_path, added_at
         FROM favorites WHERE gallery_id = ?1",
    )
    .bind(gallery_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(FavoriteEntry {
            id: r.0,
            gallery_id: r.1,
            title: r.2,
            cover_path: r.3,
            added_at: r.4,
        }),
        None => Err(Error::NotFound {
            entity: "favorite".to_string(),
            key: gallery_id.to_string(),
        }),
    }
}

/// Get total favorites count
pub async fn count(pool: &SqlitePool) -> Result<u64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM favorites")
        .fetch_one(pool)
        .await?;
    Ok(count as u64)
}