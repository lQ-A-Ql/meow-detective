CREATE TABLE file_entries (
    id TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES file_entries(id),
    data_source_id TEXT NOT NULL REFERENCES data_sources(id),
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT,
    hash_sha256 TEXT
);

CREATE INDEX idx_file_entries_parent ON file_entries(parent_id);
CREATE INDEX idx_file_entries_data_source ON file_entries(data_source_id);
CREATE INDEX idx_file_entries_path ON file_entries(path);
