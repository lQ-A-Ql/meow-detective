CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_bluefs_superblocks_inventory_source
ON ceph_bluefs_superblocks(inventory_id, data_source_id);

CREATE TABLE IF NOT EXISTS ceph_rocksdb_manifests (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_bluefs_replays(inventory_id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    active_manifest_path TEXT NOT NULL CHECK (length(active_manifest_path) > 0),
    identity_uuid TEXT CHECK (identity_uuid IS NULL OR length(identity_uuid) > 0),
    manifest_file_number INTEGER NOT NULL CHECK (manifest_file_number > 0),
    manifest_file_size INTEGER NOT NULL CHECK (manifest_file_size > 0),
    logical_edit_count INTEGER NOT NULL CHECK (logical_edit_count > 0),
    comparator_name TEXT NOT NULL CHECK (length(comparator_name) > 0),
    last_sequence INTEGER NOT NULL
        CHECK (last_sequence BETWEEN 0 AND 72057594037927935),
    next_file_number INTEGER NOT NULL CHECK (next_file_number > 0),
    log_number INTEGER NOT NULL CHECK (log_number >= 0),
    prev_log_number INTEGER NOT NULL CHECK (prev_log_number >= 0),
    max_column_family_id INTEGER NOT NULL CHECK (max_column_family_id >= 0),
    min_log_number_to_keep INTEGER
        CHECK (min_log_number_to_keep IS NULL OR min_log_number_to_keep >= 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (manifest_file_number < next_file_number),
    CHECK (log_number < next_file_number),
    CHECK (prev_log_number < next_file_number),
    CHECK (
        min_log_number_to_keep IS NULL
        OR min_log_number_to_keep < next_file_number
    ),
    FOREIGN KEY (inventory_id, data_source_id)
        REFERENCES ceph_bluefs_superblocks(inventory_id, data_source_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_manifests_source
ON ceph_rocksdb_manifests(data_source_id, inventory_id);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_manifests_file
ON ceph_rocksdb_manifests(manifest_file_number);

CREATE TABLE IF NOT EXISTS ceph_rocksdb_column_families (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_rocksdb_manifests(inventory_id) ON DELETE CASCADE,
    column_family_id INTEGER NOT NULL CHECK (column_family_id >= 0),
    name TEXT NOT NULL CHECK (length(name) > 0),
    comparator_name TEXT NOT NULL CHECK (length(comparator_name) > 0),
    dropped INTEGER NOT NULL CHECK (dropped IN (0, 1)),
    PRIMARY KEY (inventory_id, column_family_id)
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_column_families_name
ON ceph_rocksdb_column_families(inventory_id, name);

CREATE TABLE IF NOT EXISTS ceph_rocksdb_live_files (
    inventory_id TEXT NOT NULL,
    column_family_id INTEGER NOT NULL,
    level INTEGER NOT NULL CHECK (level >= 0),
    file_number INTEGER NOT NULL CHECK (file_number > 0),
    path_id INTEGER NOT NULL CHECK (path_id BETWEEN 0 AND 3),
    format TEXT NOT NULL
        CHECK (format IN ('newFile', 'newFile2', 'newFile3', 'newFile4')),
    file_size INTEGER NOT NULL CHECK (file_size > 0),
    smallest_sequence INTEGER CHECK (smallest_sequence IS NULL OR smallest_sequence >= 0),
    largest_sequence INTEGER CHECK (largest_sequence IS NULL OR largest_sequence >= 0),
    smallest_internal_key_length INTEGER NOT NULL
        CHECK (smallest_internal_key_length >= 8),
    largest_internal_key_length INTEGER NOT NULL
        CHECK (largest_internal_key_length >= 8),
    CHECK (
        (
            format = 'newFile'
            AND smallest_sequence IS NULL
            AND largest_sequence IS NULL
        )
        OR (
            format IN ('newFile2', 'newFile3', 'newFile4')
            AND
            smallest_sequence IS NOT NULL
            AND largest_sequence IS NOT NULL
            AND largest_sequence >= smallest_sequence
        )
    ),
    PRIMARY KEY (inventory_id, file_number),
    FOREIGN KEY (inventory_id, column_family_id)
        REFERENCES ceph_rocksdb_column_families(inventory_id, column_family_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_live_files_level
ON ceph_rocksdb_live_files(inventory_id, column_family_id, level, file_number);
