ALTER TABLE ceph_bluestore_rbd_headers
ADD COLUMN operation_features_hex TEXT CHECK (
    operation_features_hex IS NULL
    OR (
        length(operation_features_hex) = 16
        AND operation_features_hex NOT GLOB '*[^0-9a-f]*'
    )
);

ALTER TABLE ceph_bluestore_rbd_headers
ADD COLUMN parent_key_present INTEGER NOT NULL DEFAULT 0 CHECK (parent_key_present IN (0, 1));

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_objects_rbd_lookup
ON ceph_bluestore_objects(
    inventory_id,
    object_name,
    decoded_pool,
    object_namespace,
    snap_hex,
    object_identity_sha256
);
