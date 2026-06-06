-- 002_gallery_cache.sql: Gallery metadata cache and search history

CREATE TABLE IF NOT EXISTS galleries (
    id          INTEGER PRIMARY KEY,  -- nhentai gallery id
    media_id    TEXT    NOT NULL,
    title_en    TEXT,
    title_jp    TEXT,
    title_pretty TEXT,
    num_pages   INTEGER NOT NULL,
    num_favorites INTEGER NOT NULL DEFAULT 0,
    cover_ext   TEXT,
    tags_json   TEXT,        -- serialized JSON array of tags
    raw_json    TEXT,        -- full API response for future use
    cached_at   INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS search_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    query        TEXT    NOT NULL,
    result_count INTEGER NOT NULL DEFAULT 0,
    searched_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_search_history_searched_at ON search_history(searched_at DESC);