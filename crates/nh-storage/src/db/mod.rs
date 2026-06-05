pub mod favorites;
pub mod history;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::{debug, info};

use crate::error::{Error, Result};

/// Database handle wrapping a SQLite connection.
///
/// All database operations are synchronous (rusqlite) and should be called
/// via `tokio::task::spawn_blocking` from async code.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) a SQLite database at the given path and run migrations.
    pub fn open(db_path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        info!("Opening database at {}", db_path.display());
        let conn = Connection::open(db_path)?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open an in-memory database (useful for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run all pending migrations
    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create a migrations tracking table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );",
        )?;

        // Collect migration files (compiled into the binary)
        let migrations: &[(&str, &str)] = &[
            ("001_init.sql", include_str!("migrations/001_init.sql")),
        ];

        for (name, sql) in migrations {
            let already_applied: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM _migrations WHERE name = ?1",
                    rusqlite::params![name],
                    |row| {
                        let count: i64 = row.get(0)?;
                        Ok(count > 0)
                    },
                )
                .unwrap_or(false);

            if already_applied {
                debug!("Migration {} already applied, skipping", name);
                continue;
            }

            info!("Applying migration: {}", name);
            conn.execute_batch(sql).map_err(|e| Error::MigrationFailed {
                reason: format!("{}: {}", name, e),
            })?;

            conn.execute(
                "INSERT INTO _migrations (name) VALUES (?1)",
                rusqlite::params![name],
            )?;

            info!("Migration {} applied successfully", name);
        }

        Ok(())
    }

    /// Execute a synchronous operation on the database connection.
    ///
    /// The closure receives a `&Connection` reference and the lock is held
    /// for the duration of the closure.
    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish()
    }
}