-- Enumeration staging database schema for parallel import.
-- Each staging DB holds file_entries for one partition during import,
-- then merged into the main app.db after all partitions complete.
-- Secondary indexes are intentionally omitted during bulk insert; merge reads
-- all rows sequentially and does not need staging lookup indexes.

CREATE TABLE IF NOT EXISTS file_entries (
    id TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT,
    data_source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT,
    hash_sha256 TEXT
);

-- Metadata table for tracking partition state within the staging DB.
CREATE TABLE IF NOT EXISTS staging_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
