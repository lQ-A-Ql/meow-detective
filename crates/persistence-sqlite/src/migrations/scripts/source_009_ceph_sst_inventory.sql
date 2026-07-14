CREATE TABLE IF NOT EXISTS ceph_rocksdb_sst_inventory (
    inventory_id TEXT NOT NULL,
    file_number INTEGER NOT NULL CHECK (file_number > 0),
    column_family_id INTEGER NOT NULL CHECK (column_family_id >= 0),
    level INTEGER NOT NULL CHECK (level >= 0),
    bluefs_path TEXT NOT NULL CHECK (
        length(bluefs_path) > 0
        AND instr(bluefs_path, char(0)) = 0
    ),
    file_size INTEGER NOT NULL CHECK (file_size > 0),
    table_magic_hex TEXT NOT NULL CHECK (
        table_magic_hex = '88e241b785f4cff7'
    ),
    format_version INTEGER NOT NULL CHECK (format_version = 5),
    checksum_type TEXT NOT NULL CHECK (checksum_type = 'xxh3'),
    metaindex_offset INTEGER NOT NULL CHECK (metaindex_offset >= 0),
    metaindex_size INTEGER NOT NULL CHECK (metaindex_size > 0),
    index_offset INTEGER NOT NULL CHECK (index_offset >= 0),
    index_size INTEGER NOT NULL CHECK (index_size > 0),
    data_block_count INTEGER NOT NULL CHECK (data_block_count > 0),
    entry_count INTEGER NOT NULL CHECK (entry_count > 0),
    deletion_count INTEGER NOT NULL CHECK (deletion_count >= 0),
    merge_operand_count INTEGER NOT NULL CHECK (merge_operand_count >= 0),
    range_deletion_count INTEGER NOT NULL CHECK (range_deletion_count >= 0),
    raw_key_size INTEGER NOT NULL CHECK (raw_key_size >= 0),
    raw_value_size INTEGER NOT NULL CHECK (raw_value_size >= 0),
    data_size INTEGER NOT NULL CHECK (data_size > 0),
    properties_index_size INTEGER NOT NULL CHECK (properties_index_size > 0),
    filter_size INTEGER NOT NULL CHECK (filter_size >= 0),
    compression_name TEXT NOT NULL CHECK (length(compression_name) > 0),
    comparator_name TEXT NOT NULL CHECK (length(comparator_name) > 0),
    column_family_name TEXT NOT NULL CHECK (length(column_family_name) > 0),
    original_file_number INTEGER NOT NULL CHECK (original_file_number > 0),
    db_identity TEXT CHECK (
        db_identity IS NULL
        OR (
            length(db_identity) > 0
            AND instr(db_identity, char(0)) = 0
        )
    ),
    db_session_identity TEXT CHECK (
        db_session_identity IS NULL
        OR (
            length(db_session_identity) > 0
            AND instr(db_session_identity, char(0)) = 0
        )
    ),
    key_space_summary_version INTEGER NOT NULL DEFAULT 1
        CHECK (key_space_summary_version = 1),
    key_space_summary_json TEXT NOT NULL
        CHECK (
            json_valid(key_space_summary_json)
            AND json_extract(key_space_summary_json, '$.version') = 1
            AND json_extract(key_space_summary_json, '$.complete') = 1
            AND json_type(key_space_summary_json, '$.scannedEntries') = 'integer'
            AND json_type(
                key_space_summary_json,
                '$.scannedDecompressedBytes'
            ) = 'integer'
            AND json_type(key_space_summary_json, '$.buckets') = 'array'
        ),
    scan_complete INTEGER NOT NULL DEFAULT 1 CHECK (scan_complete = 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (inventory_id, file_number),
    FOREIGN KEY (inventory_id, column_family_id)
        REFERENCES ceph_rocksdb_column_families(inventory_id, column_family_id)
        ON DELETE CASCADE,
    FOREIGN KEY (inventory_id, file_number)
        REFERENCES ceph_rocksdb_live_files(inventory_id, file_number)
        ON DELETE CASCADE,
    CHECK (metaindex_offset + metaindex_size + 5 <= file_size),
    CHECK (index_offset + index_size + 5 <= file_size),
    CHECK (deletion_count <= entry_count),
    CHECK (merge_operand_count <= entry_count),
    CHECK (range_deletion_count <= entry_count)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_rocksdb_sst_inventory_path
ON ceph_rocksdb_sst_inventory(inventory_id, bluefs_path);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_sst_inventory_cf_level
ON ceph_rocksdb_sst_inventory(
    inventory_id,
    column_family_id,
    level,
    file_number
);
