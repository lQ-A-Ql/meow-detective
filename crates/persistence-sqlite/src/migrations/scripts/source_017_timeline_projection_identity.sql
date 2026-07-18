CREATE TABLE IF NOT EXISTS timeline_projection_meta (
    projection_key TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    inserted_count INTEGER NOT NULL DEFAULT 0,
    input_identity TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
