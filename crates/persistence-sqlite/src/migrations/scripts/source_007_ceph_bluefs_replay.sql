CREATE TABLE IF NOT EXISTS ceph_bluefs_replays (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_bluefs_superblocks(inventory_id) ON DELETE CASCADE,
    transaction_count INTEGER NOT NULL CHECK (transaction_count > 0),
    first_sequence INTEGER NOT NULL CHECK (first_sequence > 0),
    final_sequence INTEGER NOT NULL CHECK (final_sequence >= first_sequence),
    logical_bytes INTEGER NOT NULL CHECK (logical_bytes > 0),
    stop_reason TEXT NOT NULL CHECK (length(stop_reason) > 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS ceph_bluefs_directories (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluefs_replays(inventory_id) ON DELETE CASCADE,
    path TEXT NOT NULL CHECK (length(path) > 0),
    PRIMARY KEY (inventory_id, path)
);

CREATE TABLE IF NOT EXISTS ceph_bluefs_files (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluefs_replays(inventory_id) ON DELETE CASCADE,
    path TEXT NOT NULL CHECK (length(path) > 0),
    inode INTEGER NOT NULL CHECK (inode > 1),
    size INTEGER NOT NULL CHECK (size >= 0),
    mtime_seconds INTEGER NOT NULL,
    mtime_nanoseconds INTEGER NOT NULL
        CHECK (mtime_nanoseconds >= 0 AND mtime_nanoseconds < 1000000000),
    encoding INTEGER NOT NULL CHECK (encoding >= 0 AND encoding <= 255),
    content_size INTEGER NOT NULL CHECK (content_size >= 0),
    PRIMARY KEY (inventory_id, path)
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluefs_files_inode
ON ceph_bluefs_files(inventory_id, inode);

CREATE TABLE IF NOT EXISTS ceph_bluefs_file_extents (
    inventory_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    device_id INTEGER NOT NULL CHECK (device_id >= 0 AND device_id <= 255),
    offset INTEGER NOT NULL CHECK (offset >= 0),
    length INTEGER NOT NULL CHECK (length > 0),
    PRIMARY KEY (inventory_id, file_path, ordinal),
    FOREIGN KEY (inventory_id, file_path)
        REFERENCES ceph_bluefs_files(inventory_id, path)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluefs_file_extents_device
ON ceph_bluefs_file_extents(inventory_id, device_id, offset);
