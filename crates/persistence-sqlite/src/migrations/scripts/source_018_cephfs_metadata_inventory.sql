CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_objects_pool_identity
ON ceph_bluestore_objects(
    inventory_id,
    decoded_pool,
    object_identity_sha256
);

CREATE TABLE IF NOT EXISTS ceph_fs_metadata_inventories (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE,
    filesystem_id INTEGER NOT NULL CHECK (filesystem_id >= 0),
    fsmap_epoch INTEGER NOT NULL CHECK (fsmap_epoch BETWEEN 0 AND 4294967295),
    metadata_pool_id INTEGER NOT NULL CHECK (metadata_pool_id >= 0),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    classifier_profile TEXT NOT NULL CHECK (classifier_profile = 'cephfs-metadata-v1'),
    source_semantic_sha256 TEXT NOT NULL CHECK (
        length(source_semantic_sha256) = 64
        AND source_semantic_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    inventory_sha256 TEXT NOT NULL CHECK (
        length(inventory_sha256) = 64
        AND inventory_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    object_count INTEGER NOT NULL CHECK (object_count >= 0),
    unknown_object_count INTEGER NOT NULL CHECK (
        unknown_object_count BETWEEN 0 AND object_count
    ),
    complete INTEGER NOT NULL CHECK (complete = 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (filesystem_identity, inventory_id)
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_metadata_inventories_source
ON ceph_fs_metadata_inventories(data_source_id, filesystem_identity, fsmap_epoch);

CREATE TABLE IF NOT EXISTS ceph_fs_metadata_objects (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL CHECK (
        length(object_identity_sha256) = 64
        AND object_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    locator TEXT NOT NULL,
    candidate_mask INTEGER NOT NULL CHECK (candidate_mask BETWEEN 0 AND 63),
    classification_state TEXT NOT NULL CHECK (
        classification_state IN ('candidate', 'classified', 'metadata_only')
    ),
    classifier_rule TEXT NOT NULL CHECK (
        length(classifier_rule) BETWEEN 1 AND 64
        AND classifier_rule NOT GLOB '*[^0-9A-Za-z._-]*'
    ),
    record_sha256 TEXT NOT NULL CHECK (
        length(record_sha256) = 64
        AND record_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (filesystem_identity, inventory_id, object_identity_sha256),
    UNIQUE (filesystem_identity, inventory_id, locator),
    FOREIGN KEY (filesystem_identity, inventory_id)
        REFERENCES ceph_fs_metadata_inventories(filesystem_identity, inventory_id)
        ON DELETE CASCADE,
    FOREIGN KEY (inventory_id, object_identity_sha256)
        REFERENCES ceph_bluestore_objects(inventory_id, object_identity_sha256)
        ON DELETE CASCADE,
    CHECK (
        (candidate_mask = 0 AND classification_state IN ('classified', 'metadata_only'))
        OR (candidate_mask <> 0 AND classification_state = 'candidate')
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_metadata_objects_locator
ON ceph_fs_metadata_objects(filesystem_identity, locator);
