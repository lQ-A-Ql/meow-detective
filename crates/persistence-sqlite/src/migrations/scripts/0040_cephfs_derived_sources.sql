CREATE TABLE IF NOT EXISTS ceph_fs_derived_lineage (
    derived_data_source_id TEXT PRIMARY KEY NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE,
    parent_cluster_id TEXT NOT NULL
        REFERENCES data_source_clusters(id) ON DELETE RESTRICT,
    cluster_identity TEXT NOT NULL,
    filesystem_identity TEXT NOT NULL,
    filesystem_id INTEGER NOT NULL CHECK (filesystem_id >= 0),
    filesystem_name TEXT NOT NULL,
    fsmap_epoch INTEGER NOT NULL CHECK (fsmap_epoch BETWEEN 1 AND 4294967295),
    mdsmap_epoch INTEGER NOT NULL CHECK (mdsmap_epoch BETWEEN 1 AND 4294967295),
    descriptor_state TEXT NOT NULL CHECK (descriptor_state = 'present'),
    metadata_pool_id INTEGER NOT NULL CHECK (metadata_pool_id >= 0),
    expected_replica_count INTEGER NOT NULL CHECK (expected_replica_count > 0),
    namespace_input_sha256 TEXT NOT NULL CHECK (
        length(namespace_input_sha256) = 64
        AND namespace_input_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    namespace_projection_sha256 TEXT NOT NULL CHECK (
        length(namespace_projection_sha256) = 64
        AND namespace_projection_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    namespace_schema_version INTEGER NOT NULL CHECK (namespace_schema_version = 1),
    decoder_profile TEXT NOT NULL CHECK (decoder_profile = 'cephfs-namespace-v1'),
    journal_boundary_sha256 TEXT CHECK (
        journal_boundary_sha256 IS NULL
        OR (
            length(journal_boundary_sha256) = 64
            AND journal_boundary_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    lineage_fingerprint TEXT NOT NULL CHECK (
        length(lineage_fingerprint) = 64
        AND lineage_fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (parent_cluster_id, filesystem_identity)
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_derived_lineage_cluster
ON ceph_fs_derived_lineage(parent_cluster_id, derived_data_source_id);

CREATE TABLE IF NOT EXISTS ceph_fs_derived_pools (
    derived_data_source_id TEXT NOT NULL
        REFERENCES ceph_fs_derived_lineage(derived_data_source_id) ON DELETE CASCADE,
    pool_id INTEGER NOT NULL CHECK (pool_id >= 0),
    role TEXT NOT NULL CHECK (role IN ('metadata', 'data')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (derived_data_source_id, pool_id),
    UNIQUE (derived_data_source_id, role, ordinal),
    CHECK ((role = 'metadata' AND ordinal = 0) OR role = 'data')
);

CREATE TABLE IF NOT EXISTS ceph_fs_derived_pool_sources (
    derived_data_source_id TEXT NOT NULL,
    pool_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE RESTRICT,
    inventory_id TEXT NOT NULL,
    PRIMARY KEY (derived_data_source_id, pool_id, ordinal),
    UNIQUE (derived_data_source_id, pool_id, source_data_source_id),
    UNIQUE (derived_data_source_id, pool_id, inventory_id),
    FOREIGN KEY (derived_data_source_id, pool_id)
        REFERENCES ceph_fs_derived_pools(derived_data_source_id, pool_id)
        ON DELETE CASCADE,
    CHECK (source_data_source_id <> derived_data_source_id)
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_derived_pool_sources_source
ON ceph_fs_derived_pool_sources(source_data_source_id, derived_data_source_id);

CREATE TABLE IF NOT EXISTS ceph_fs_derived_map_provenance (
    derived_data_source_id TEXT NOT NULL
        REFERENCES ceph_fs_derived_lineage(derived_data_source_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE RESTRICT,
    inventory_id TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    raw_fsmap_sha256 TEXT NOT NULL CHECK (
        length(raw_fsmap_sha256) = 64
        AND raw_fsmap_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    raw_mdsmap_sha256 TEXT NOT NULL CHECK (
        length(raw_mdsmap_sha256) = 64
        AND raw_mdsmap_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (derived_data_source_id, ordinal),
    UNIQUE (derived_data_source_id, source_data_source_id, inventory_id)
);
