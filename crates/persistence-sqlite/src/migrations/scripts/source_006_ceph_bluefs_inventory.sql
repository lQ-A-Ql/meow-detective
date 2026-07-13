CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_osd_inventory_bluefs_binding
ON ceph_osd_inventory(id, data_source_id, osd_uuid);

CREATE TABLE IF NOT EXISTS ceph_bluefs_superblocks (
    inventory_id TEXT PRIMARY KEY NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    bluefs_uuid TEXT NOT NULL,
    osd_uuid TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    block_size INTEGER NOT NULL CHECK (block_size > 0),
    crc32c INTEGER NOT NULL CHECK (crc32c >= 0 AND crc32c <= 4294967295),
    struct_version INTEGER NOT NULL CHECK (struct_version >= 0 AND struct_version <= 255),
    struct_compat_version INTEGER NOT NULL
        CHECK (struct_compat_version >= 0 AND struct_compat_version <= 255),
    log_inode INTEGER NOT NULL CHECK (log_inode >= 0),
    log_size INTEGER NOT NULL CHECK (log_size >= 0),
    log_mtime_seconds INTEGER NOT NULL,
    log_mtime_nanoseconds INTEGER NOT NULL
        CHECK (log_mtime_nanoseconds >= 0 AND log_mtime_nanoseconds < 1000000000),
    log_encoding INTEGER NOT NULL CHECK (log_encoding >= 0 AND log_encoding <= 255),
    log_content_size INTEGER NOT NULL CHECK (log_content_size >= 0),
    shared_bdev INTEGER,
    dedicated_db INTEGER CHECK (dedicated_db IS NULL OR dedicated_db IN (0, 1)),
    dedicated_wal INTEGER CHECK (dedicated_wal IS NULL OR dedicated_wal IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (inventory_id, data_source_id, osd_uuid)
        REFERENCES ceph_osd_inventory(id, data_source_id, osd_uuid)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluefs_superblocks_source
ON ceph_bluefs_superblocks(data_source_id, osd_uuid);

CREATE TABLE IF NOT EXISTS ceph_bluefs_log_extents (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluefs_superblocks(inventory_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    device_id INTEGER NOT NULL CHECK (device_id >= 0 AND device_id <= 255),
    offset INTEGER NOT NULL CHECK (offset >= 0),
    length INTEGER NOT NULL CHECK (length > 0),
    PRIMARY KEY (inventory_id, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluefs_log_extents_device
ON ceph_bluefs_log_extents(inventory_id, device_id, offset);
