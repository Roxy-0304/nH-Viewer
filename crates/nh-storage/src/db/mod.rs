pub mod favorites;
pub mod gallery_cache;
pub mod history;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tracing::{debug, info};

use crate::error::{Error, Result};

/// Database handle wrapping a sqlx SQLite connection pool.
///
/// All database operations are fully async via sqlx.
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Open (or create) a SQLite database at the given path and run migrations.
    pub async fn open(db_path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        info!("Opening database at {}", db_path.display());

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Open an in-memory database (useful for testing).
    pub async fn open_memory() -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Run all pending migrations.
    async fn run_migrations(&self) -> Result<()> {
        // Create a migrations tracking table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );",
        )
        .execute(&self.pool)
        .await?;

        // Collect migration files (compiled into the binary)
        let migrations: &[(&str, &str)] = &[
            ("001_init.sql", include_str!("migrations/001_init.sql")),
            (
                "002_gallery_cache.sql",
                include_str!("migrations/002_gallery_cache.sql"),
            ),
        ];

        for (name, sql) in migrations {
            let already_applied: bool =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _migrations WHERE name = ?1")
                    .bind(name)
                    .fetch_one(&self.pool)
                    .await
                    .map(|count| count > 0)
                    .unwrap_or(false);

            if already_applied {
                debug!("Migration {} already applied, skipping", name);
                continue;
            }

            info!("Applying migration: {}", name);
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| Error::MigrationFailed {
                    reason: format!("{}: {}", name, e),
                })?;

            sqlx::query("INSERT OR IGNORE INTO _migrations (name) VALUES (?1)")
                .bind(name)
                .execute(&self.pool)
                .await?;

            info!("Migration {} applied successfully", name);
        }

        Ok(())
    }

    /// Get a reference to the underlying connection pool.
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish()
    }
}
