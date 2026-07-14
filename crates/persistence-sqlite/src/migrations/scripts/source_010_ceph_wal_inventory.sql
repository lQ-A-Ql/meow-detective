ALTER TABLE ceph_rocksdb_column_families
ADD COLUMN log_number INTEGER
    CHECK (log_number IS NULL OR log_number >= 0);

CREATE TABLE IF NOT EXISTS ceph_rocksdb_wal_files (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_rocksdb_manifests(inventory_id) ON DELETE CASCADE,
    wal_number INTEGER NOT NULL CHECK (wal_number > 0),
    bluefs_path TEXT NOT NULL CHECK (
        length(bluefs_path) > 0
        AND instr(bluefs_path, char(0)) = 0
    ),
    post_manifest INTEGER NOT NULL CHECK (post_manifest IN (0, 1)),
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    logical_record_count INTEGER NOT NULL CHECK (logical_record_count >= 0),
    empty_batch_count INTEGER NOT NULL CHECK (empty_batch_count >= 0),
    mutation_count INTEGER NOT NULL CHECK (mutation_count >= 0),
    auxiliary_record_count INTEGER NOT NULL CHECK (auxiliary_record_count >= 0),
    logical_payload_bytes INTEGER NOT NULL CHECK (logical_payload_bytes >= 0),
    fragment_count INTEGER NOT NULL CHECK (fragment_count >= 0),
    first_sequence INTEGER CHECK (
        first_sequence IS NULL
        OR first_sequence BETWEEN 0 AND 72057594037927935
    ),
    last_sequence INTEGER CHECK (
        last_sequence IS NULL
        OR last_sequence BETWEEN 0 AND 72057594037927935
    ),
    first_record_offset INTEGER CHECK (
        first_record_offset IS NULL OR first_record_offset >= 0
    ),
    last_record_offset INTEGER CHECK (
        last_record_offset IS NULL OR last_record_offset >= 0
    ),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (inventory_id, wal_number),
    UNIQUE (inventory_id, bluefs_path),
    CHECK (empty_batch_count <= logical_record_count),
    CHECK (
        (
            logical_record_count = 0
            AND empty_batch_count = 0
            AND mutation_count = 0
            AND auxiliary_record_count = 0
            AND logical_payload_bytes = 0
            AND fragment_count = 0
            AND first_sequence IS NULL
            AND last_sequence IS NULL
            AND first_record_offset IS NULL
            AND last_record_offset IS NULL
        )
        OR (
            logical_record_count > 0
            AND logical_payload_bytes >= logical_record_count * 12
            AND fragment_count >= logical_record_count
            AND first_sequence IS NOT NULL
            AND last_sequence IS NOT NULL
            AND first_sequence <= last_sequence
            AND first_record_offset IS NOT NULL
            AND last_record_offset IS NOT NULL
            AND first_record_offset <= last_record_offset
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_wal_files_path
ON ceph_rocksdb_wal_files(inventory_id, bluefs_path);

CREATE TABLE IF NOT EXISTS ceph_rocksdb_wal_records (
    inventory_id TEXT NOT NULL,
    wal_number INTEGER NOT NULL CHECK (wal_number > 0),
    record_ordinal INTEGER NOT NULL CHECK (record_ordinal >= 0),
    physical_offset INTEGER NOT NULL CHECK (physical_offset >= 0),
    fragment_count INTEGER NOT NULL CHECK (fragment_count > 0),
    recyclable_log_number INTEGER CHECK (
        recyclable_log_number IS NULL
        OR recyclable_log_number BETWEEN 0 AND 4294967295
    ),
    batch_sequence INTEGER NOT NULL
        CHECK (batch_sequence BETWEEN 0 AND 72057594037927935),
    mutation_count INTEGER NOT NULL CHECK (mutation_count >= 0),
    auxiliary_record_count INTEGER NOT NULL CHECK (auxiliary_record_count >= 0),
    first_mutation_sequence INTEGER CHECK (
        first_mutation_sequence IS NULL
        OR first_mutation_sequence BETWEEN 0 AND 72057594037927935
    ),
    last_mutation_sequence INTEGER CHECK (
        last_mutation_sequence IS NULL
        OR last_mutation_sequence BETWEEN 0 AND 72057594037927935
    ),
    PRIMARY KEY (inventory_id, wal_number, record_ordinal),
    UNIQUE (inventory_id, wal_number, physical_offset),
    FOREIGN KEY (inventory_id, wal_number)
        REFERENCES ceph_rocksdb_wal_files(inventory_id, wal_number)
        ON DELETE CASCADE,
    CHECK (
        (
            mutation_count = 0
            AND first_mutation_sequence IS NULL
            AND last_mutation_sequence IS NULL
        )
        OR (
            mutation_count > 0
            AND first_mutation_sequence = batch_sequence
            AND last_mutation_sequence = batch_sequence + mutation_count - 1
            AND last_mutation_sequence <= 72057594037927935
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_wal_records_sequence
ON ceph_rocksdb_wal_records(
    inventory_id,
    first_mutation_sequence,
    last_mutation_sequence
);
