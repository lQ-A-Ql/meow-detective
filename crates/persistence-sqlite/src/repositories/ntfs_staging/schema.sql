CREATE TABLE IF NOT EXISTS ntfs_entries (
    record_num INTEGER PRIMARY KEY CHECK(record_num BETWEEN 0 AND 281474976710655),
    parent_record_num INTEGER,
    sequence_num INTEGER NOT NULL,
    name TEXT NOT NULL,
    entry_type INTEGER NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL,
    hidden INTEGER NOT NULL,
    system INTEGER NOT NULL,
    read_only INTEGER NOT NULL,
    archive INTEGER NOT NULL,
    encrypted INTEGER,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT
);
CREATE TABLE IF NOT EXISTS ntfs_paths (
    record_num INTEGER PRIMARY KEY CHECK(record_num BETWEEN 0 AND 281474976710655),
    resolved_path TEXT NOT NULL,
    catalog_name TEXT NOT NULL,
    catalog_parent_record_num INTEGER,
    resolution_status INTEGER NOT NULL CHECK(resolution_status BETWEEN 0 AND 3)
);
