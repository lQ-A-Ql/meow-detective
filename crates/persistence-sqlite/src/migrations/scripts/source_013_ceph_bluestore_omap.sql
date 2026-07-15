CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_osd_inventory_id_source
ON ceph_osd_inventory(id, data_source_id);

CREATE TABLE IF NOT EXISTS ceph_bluestore_omap_scans (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    decode_profile TEXT NOT NULL DEFAULT 'omap-rbd-v1'
        CHECK (decode_profile = 'omap-rbd-v1'),
    sharding_sha256 TEXT NOT NULL CHECK (
        length(sharding_sha256) = 64
        AND sharding_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    latest_state_sha256 TEXT NOT NULL CHECK (
        length(latest_state_sha256) = 64
        AND latest_state_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    semantic_sha256 TEXT NOT NULL CHECK (
        length(semantic_sha256) = 64
        AND semantic_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    omap_sha256 TEXT NOT NULL CHECK (
        length(omap_sha256) = 64
        AND omap_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    scope_count INTEGER NOT NULL CHECK (scope_count >= 0),
    directory_mapping_count INTEGER NOT NULL CHECK (directory_mapping_count >= 0),
    rbd_header_count INTEGER NOT NULL CHECK (rbd_header_count >= 0),
    profile_complete INTEGER NOT NULL DEFAULT 1 CHECK (profile_complete = 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (inventory_id, data_source_id)
        REFERENCES ceph_osd_inventory(id, data_source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_omap_scans_source
ON ceph_bluestore_omap_scans(data_source_id, inventory_id);

CREATE TABLE IF NOT EXISTS ceph_bluestore_omap_scopes (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluestore_omap_scans(inventory_id) ON DELETE CASCADE,
    scope_identity TEXT NOT NULL CHECK (
        length(scope_identity) > 0
        AND instr(scope_identity, char(0)) = 0
    ),
    key_family TEXT NOT NULL CHECK (
        key_family IN ('bulk', 'pgMeta', 'perPool', 'perPg')
    ),
    pool_kind TEXT NOT NULL CHECK (
        pool_kind IN ('none', 'perPool', 'perPg')
    ),
    pool_value_i64 INTEGER,
    pool_value_hex TEXT CHECK (
        pool_value_hex IS NULL
        OR (
            length(pool_value_hex) = 16
            AND pool_value_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    hash INTEGER CHECK (hash IS NULL OR hash BETWEEN 0 AND 4294967295),
    nid_hex TEXT NOT NULL CHECK (
        length(nid_hex) = 16
        AND nid_hex NOT GLOB '*[^0-9a-f]*'
    ),
    owner_nid_hex TEXT CHECK (
        owner_nid_hex IS NULL
        OR (
            length(owner_nid_hex) = 16
            AND owner_nid_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    owner_family TEXT CHECK (
        owner_family IS NULL
        OR owner_family IN ('bulk', 'pgMeta', 'perPool', 'perPg')
    ),
    owner_kind TEXT CHECK (
        owner_kind IS NULL OR owner_kind IN ('rbdDirectory', 'rbdHeader')
    ),
    owner_image_id TEXT CHECK (
        owner_image_id IS NULL
        OR (
            length(owner_image_id) BETWEEN 1 AND 4096
            AND instr(owner_image_id, char(0)) = 0
        )
    ),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    recognized_entry_count INTEGER NOT NULL CHECK (
        recognized_entry_count >= 0 AND recognized_entry_count <= entry_count
    ),
    PRIMARY KEY (inventory_id, scope_identity),
    CHECK (
        (key_family IN ('bulk', 'pgMeta')
            AND pool_kind = 'none'
            AND pool_value_i64 IS NULL
            AND pool_value_hex IS NULL
            AND hash IS NULL)
        OR (key_family = 'perPool'
            AND pool_kind = 'perPool'
            AND pool_value_i64 IS NOT NULL
            AND pool_value_hex IS NULL
            AND hash IS NULL)
        OR (key_family = 'perPg'
            AND pool_kind = 'perPg'
            AND pool_value_i64 IS NULL
            AND pool_value_hex IS NOT NULL
            AND hash IS NOT NULL)
    ),
    CHECK (
        (owner_nid_hex IS NULL
            AND owner_family IS NULL
            AND owner_kind IS NULL
            AND owner_image_id IS NULL)
        OR (owner_nid_hex IS NOT NULL
            AND owner_family = key_family
            AND owner_kind = 'rbdDirectory'
            AND owner_image_id IS NULL)
        OR (owner_nid_hex IS NOT NULL
            AND owner_family = key_family
            AND owner_kind = 'rbdHeader'
            AND owner_image_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_omap_scopes_family
ON ceph_bluestore_omap_scopes(
    inventory_id,
    key_family,
    pool_kind,
    pool_value_i64,
    pool_value_hex,
    hash,
    nid_hex
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_omap_scopes_owner
ON ceph_bluestore_omap_scopes(
    inventory_id,
    owner_family,
    owner_nid_hex,
    owner_kind
)
WHERE owner_nid_hex IS NOT NULL;

CREATE TABLE IF NOT EXISTS ceph_bluestore_rbd_directory (
    inventory_id TEXT NOT NULL,
    scope_identity TEXT NOT NULL,
    owner_nid_hex TEXT NOT NULL CHECK (
        length(owner_nid_hex) = 16
        AND owner_nid_hex NOT GLOB '*[^0-9a-f]*'
    ),
    image_name TEXT NOT NULL CHECK (
        length(image_name) BETWEEN 1 AND 4096
        AND instr(image_name, char(0)) = 0
    ),
    image_id TEXT NOT NULL CHECK (
        length(image_id) BETWEEN 1 AND 4096
        AND instr(image_id, char(0)) = 0
    ),
    bidirectional INTEGER NOT NULL CHECK (bidirectional IN (0, 1)),
    PRIMARY KEY (inventory_id, scope_identity, image_name),
    UNIQUE (inventory_id, scope_identity, image_id),
    FOREIGN KEY (inventory_id, scope_identity)
        REFERENCES ceph_bluestore_omap_scopes(inventory_id, scope_identity)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_rbd_directory_image_id
ON ceph_bluestore_rbd_directory(inventory_id, image_id);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_rbd_directory_image_name
ON ceph_bluestore_rbd_directory(inventory_id, image_name);

CREATE TABLE IF NOT EXISTS ceph_bluestore_rbd_headers (
    inventory_id TEXT NOT NULL,
    scope_identity TEXT NOT NULL,
    owner_nid_hex TEXT NOT NULL CHECK (
        length(owner_nid_hex) = 16
        AND owner_nid_hex NOT GLOB '*[^0-9a-f]*'
    ),
    image_id TEXT NOT NULL CHECK (
        length(image_id) BETWEEN 1 AND 4096
        AND instr(image_id, char(0)) = 0
    ),
    size_hex TEXT CHECK (
        size_hex IS NULL
        OR (
            length(size_hex) = 16
            AND size_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    object_order INTEGER CHECK (object_order IS NULL OR object_order BETWEEN 0 AND 63),
    features_hex TEXT CHECK (
        features_hex IS NULL
        OR (
            length(features_hex) = 16
            AND features_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    object_prefix TEXT CHECK (
        object_prefix IS NULL
        OR (
            length(object_prefix) BETWEEN 1 AND 4096
            AND instr(object_prefix, char(0)) = 0
        )
    ),
    stripe_unit_hex TEXT CHECK (
        stripe_unit_hex IS NULL
        OR (
            length(stripe_unit_hex) = 16
            AND stripe_unit_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    stripe_count_hex TEXT CHECK (
        stripe_count_hex IS NULL
        OR (
            length(stripe_count_hex) = 16
            AND stripe_count_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    data_pool_id INTEGER,
    PRIMARY KEY (inventory_id, image_id),
    UNIQUE (inventory_id, scope_identity),
    FOREIGN KEY (inventory_id, scope_identity)
        REFERENCES ceph_bluestore_omap_scopes(inventory_id, scope_identity)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_rbd_headers_owner
ON ceph_bluestore_rbd_headers(inventory_id, owner_nid_hex, image_id);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_rbd_headers_data_pool
ON ceph_bluestore_rbd_headers(inventory_id, data_pool_id, image_id);
