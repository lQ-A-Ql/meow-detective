CREATE TABLE IF NOT EXISTS ceph_rbd_derived_lineage (
    derived_data_source_id TEXT PRIMARY KEY NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE
        CHECK (
            length(trim(derived_data_source_id)) > 0
            AND instr(derived_data_source_id, char(0)) = 0
        ),
    parent_cluster_id TEXT NOT NULL
        REFERENCES data_source_clusters(id) ON DELETE RESTRICT
        CHECK (
            length(trim(parent_cluster_id)) > 0
            AND instr(parent_cluster_id, char(0)) = 0
        ),
    image_name TEXT NOT NULL
        CHECK (length(trim(image_name)) > 0 AND instr(image_name, char(0)) = 0),
    image_id TEXT NOT NULL
        CHECK (length(trim(image_id)) > 0 AND instr(image_id, char(0)) = 0),
    object_prefix TEXT NOT NULL
        CHECK (length(trim(object_prefix)) > 0 AND instr(object_prefix, char(0)) = 0),
    image_size INTEGER NOT NULL CHECK (image_size > 0),
    object_order INTEGER NOT NULL CHECK (object_order BETWEEN 12 AND 25),
    features TEXT NOT NULL
        CHECK (length(features) = 16 AND features NOT GLOB '*[^0-9a-f]*'),
    stripe_unit INTEGER NOT NULL CHECK (stripe_unit >= 0),
    stripe_count INTEGER NOT NULL CHECK (stripe_count >= 0),
    data_pool_id INTEGER NOT NULL CHECK (data_pool_id >= 0),
    scope_identity TEXT NOT NULL
        CHECK (length(trim(scope_identity)) > 0 AND instr(scope_identity, char(0)) = 0),
    operation_features TEXT NOT NULL
        CHECK (
            length(operation_features) = 16
            AND operation_features NOT GLOB '*[^0-9a-f]*'
        ),
    has_parent INTEGER NOT NULL CHECK (has_parent IN (0, 1)),
    snapshot_id TEXT
        CHECK (
            snapshot_id IS NULL
            OR (
                length(snapshot_id) = 16
                AND snapshot_id NOT GLOB '*[^0-9a-f]*'
            )
        ),
    encrypted INTEGER NOT NULL CHECK (encrypted IN (0, 1)),
    expected_replica_count INTEGER NOT NULL CHECK (expected_replica_count > 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ceph_rbd_derived_lineage_cluster
ON ceph_rbd_derived_lineage(parent_cluster_id, derived_data_source_id);

CREATE TABLE IF NOT EXISTS ceph_rbd_derived_replicas (
    derived_data_source_id TEXT NOT NULL
        REFERENCES ceph_rbd_derived_lineage(derived_data_source_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE RESTRICT
        CHECK (
            length(trim(source_data_source_id)) > 0
            AND instr(source_data_source_id, char(0)) = 0
            AND source_data_source_id <> derived_data_source_id
        ),
    inventory_id TEXT NOT NULL
        CHECK (length(trim(inventory_id)) > 0 AND instr(inventory_id, char(0)) = 0),
    osd_id INTEGER NOT NULL CHECK (osd_id >= 0),
    PRIMARY KEY (derived_data_source_id, ordinal),
    UNIQUE (derived_data_source_id, source_data_source_id),
    UNIQUE (derived_data_source_id, inventory_id),
    UNIQUE (derived_data_source_id, osd_id)
);

CREATE INDEX IF NOT EXISTS idx_ceph_rbd_derived_replicas_source
ON ceph_rbd_derived_replicas(source_data_source_id, derived_data_source_id);
