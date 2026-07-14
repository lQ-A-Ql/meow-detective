CREATE TABLE IF NOT EXISTS ceph_rocksdb_latest_state (
    inventory_id TEXT NOT NULL CHECK (
        length(inventory_id) > 0
        AND instr(inventory_id, char(0)) = 0
    ),
    column_family_id INTEGER NOT NULL CHECK (column_family_id >= 0),
    column_family_name TEXT NOT NULL CHECK (
        length(column_family_name) > 0
        AND instr(column_family_name, char(0)) = 0
    ),
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    sharding_sha256 TEXT NOT NULL CHECK (
        length(sharding_sha256) = 64
        AND sharding_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    point_mutation_count INTEGER NOT NULL CHECK (point_mutation_count >= 0),
    sst_point_mutation_count INTEGER NOT NULL CHECK (sst_point_mutation_count >= 0),
    wal_point_mutation_count INTEGER NOT NULL CHECK (wal_point_mutation_count >= 0),
    range_mutation_count INTEGER NOT NULL CHECK (range_mutation_count >= 0),
    sst_range_mutation_count INTEGER NOT NULL CHECK (sst_range_mutation_count >= 0),
    wal_range_mutation_count INTEGER NOT NULL CHECK (wal_range_mutation_count >= 0),
    latest_value_count INTEGER NOT NULL CHECK (latest_value_count >= 0),
    deleted_key_count INTEGER NOT NULL CHECK (deleted_key_count >= 0),
    delete_decision_count INTEGER NOT NULL CHECK (delete_decision_count >= 0),
    single_delete_decision_count INTEGER NOT NULL
        CHECK (single_delete_decision_count >= 0),
    range_delete_decision_count INTEGER NOT NULL
        CHECK (range_delete_decision_count >= 0),
    merge_resolved_count INTEGER NOT NULL CHECK (merge_resolved_count >= 0),
    merge_operand_count INTEGER NOT NULL CHECK (merge_operand_count >= 0),
    range_hidden_version_count INTEGER NOT NULL
        CHECK (range_hidden_version_count >= 0),
    smallest_sequence INTEGER CHECK (
        smallest_sequence IS NULL
        OR smallest_sequence BETWEEN 0 AND 72057594037927935
    ),
    largest_sequence INTEGER CHECK (
        largest_sequence IS NULL
        OR largest_sequence BETWEEN 0 AND 72057594037927935
    ),
    point_sha256 TEXT NOT NULL CHECK (
        length(point_sha256) = 64
        AND point_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    range_sha256 TEXT NOT NULL CHECK (
        length(range_sha256) = 64
        AND range_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    latest_state_sha256 TEXT NOT NULL CHECK (
        length(latest_state_sha256) = 64
        AND latest_state_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    scan_complete INTEGER NOT NULL DEFAULT 1 CHECK (scan_complete = 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (inventory_id, column_family_id),
    FOREIGN KEY (inventory_id, column_family_id)
        REFERENCES ceph_rocksdb_column_families(inventory_id, column_family_id)
        ON DELETE CASCADE,
    CHECK (
        point_mutation_count
        = sst_point_mutation_count + wal_point_mutation_count
    ),
    CHECK (
        range_mutation_count
        = sst_range_mutation_count + wal_range_mutation_count
    ),
    CHECK (
        deleted_key_count
        = delete_decision_count
        + single_delete_decision_count
        + range_delete_decision_count
    ),
    CHECK (latest_value_count + deleted_key_count <= point_mutation_count),
    CHECK (merge_resolved_count <= latest_value_count),
    CHECK (merge_resolved_count <= merge_operand_count),
    CHECK (merge_operand_count <= point_mutation_count),
    CHECK (range_hidden_version_count <= point_mutation_count),
    CHECK (
        range_mutation_count > 0
        OR (
            range_delete_decision_count = 0
            AND range_hidden_version_count = 0
        )
    ),
    CHECK (
        (
            point_mutation_count + range_mutation_count = 0
            AND smallest_sequence IS NULL
            AND largest_sequence IS NULL
        )
        OR (
            point_mutation_count + range_mutation_count > 0
            AND smallest_sequence IS NOT NULL
            AND largest_sequence IS NOT NULL
            AND smallest_sequence <= largest_sequence
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_rocksdb_latest_state_inventory
ON ceph_rocksdb_latest_state(inventory_id, column_family_name, column_family_id);
