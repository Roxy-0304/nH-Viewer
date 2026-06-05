-- 001_init.sql: Initial schema for nh-viewer

CREATE TABLE IF NOT EXISTS history (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    gallery_id INTEGER NOT NULL,
    title     TEXT    NOT NULL,
    cover_path TEXT,
    viewed_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    page      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_history_gallery_id ON history(gallery_id);
CREATE INDEX IF NOT EXISTS idx_history_viewed_at  ON history(viewed_at DESC);

CREATE TABLE IF NOT EXISTS favorites (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    gallery_id INTEGER NOT NULL UNIQUE,
    title      TEXT    NOT NULL,
    cover_path TEXT,
    added_at   INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_favorites_gallery_id ON favorites(gallery_id);
CREATE INDEX IF NOT EXISTS idx_favorites_added_at   ON favorites(added_at DESC);