CREATE TABLE IF NOT EXISTS ceph_bluestore_semantic_scans (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_rocksdb_manifests(inventory_id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    decode_profile TEXT NOT NULL DEFAULT 'scox-v1'
        CHECK (decode_profile = 'scox-v1'),
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
    s_latest_count INTEGER NOT NULL CHECK (s_latest_count >= 0),
    s_decoded_count INTEGER NOT NULL CHECK (s_decoded_count >= 0),
    s_deferred_count INTEGER NOT NULL CHECK (s_deferred_count >= 0),
    c_latest_count INTEGER NOT NULL CHECK (c_latest_count >= 0),
    c_decoded_count INTEGER NOT NULL CHECK (c_decoded_count >= 0),
    c_deferred_count INTEGER NOT NULL CHECK (c_deferred_count >= 0),
    o_latest_count INTEGER NOT NULL CHECK (o_latest_count >= 0),
    o_decoded_count INTEGER NOT NULL CHECK (o_decoded_count >= 0),
    o_deferred_count INTEGER NOT NULL CHECK (o_deferred_count >= 0),
    x_latest_count INTEGER NOT NULL CHECK (x_latest_count >= 0),
    x_decoded_count INTEGER NOT NULL CHECK (x_decoded_count >= 0),
    x_deferred_count INTEGER NOT NULL CHECK (x_deferred_count >= 0),
    collection_count INTEGER NOT NULL CHECK (collection_count >= 0),
    object_count INTEGER NOT NULL CHECK (object_count >= 0),
    blob_count INTEGER NOT NULL CHECK (blob_count >= 0),
    onode_shard_count INTEGER NOT NULL CHECK (onode_shard_count >= 0),
    logical_extent_count INTEGER NOT NULL CHECK (logical_extent_count >= 0),
    physical_extent_count INTEGER NOT NULL CHECK (physical_extent_count >= 0),
    checksum_chunk_count INTEGER NOT NULL CHECK (checksum_chunk_count >= 0),
    shared_blob_count INTEGER NOT NULL CHECK (shared_blob_count >= 0),
    shared_ref_extent_count INTEGER NOT NULL CHECK (shared_ref_extent_count >= 0),
    profile_complete INTEGER NOT NULL DEFAULT 1 CHECK (profile_complete = 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (s_latest_count = s_decoded_count + s_deferred_count),
    CHECK (c_latest_count = c_decoded_count + c_deferred_count),
    CHECK (o_latest_count = o_decoded_count + o_deferred_count),
    CHECK (x_latest_count = x_decoded_count + x_deferred_count)
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_super (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    nid_max INTEGER CHECK (nid_max IS NULL OR nid_max >= 0),
    blobid_max INTEGER CHECK (blobid_max IS NULL OR blobid_max >= 0),
    min_alloc_size INTEGER CHECK (min_alloc_size IS NULL OR min_alloc_size > 0),
    ondisk_format INTEGER,
    min_compat_ondisk_format INTEGER,
    per_pool_omap TEXT CHECK (
        per_pool_omap IS NULL OR per_pool_omap IN ('bulk', 'perPool', 'perPg')
    ),
    freelist_type TEXT CHECK (
        freelist_type IS NULL
        OR (length(freelist_type) > 0 AND instr(freelist_type, char(0)) = 0)
    ),
    observed_count INTEGER NOT NULL CHECK (observed_count >= 0),
    deferred_count INTEGER NOT NULL CHECK (deferred_count >= 0),
    CHECK (deferred_count <= observed_count)
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_collections (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    collection_identity TEXT NOT NULL CHECK (
        length(collection_identity) > 0
        AND instr(collection_identity, char(0)) = 0
    ),
    kind TEXT NOT NULL CHECK (kind IN ('meta', 'head', 'temp')),
    pool INTEGER CHECK (pool IS NULL OR pool >= 0),
    seed INTEGER CHECK (seed IS NULL OR seed BETWEEN 0 AND 4294967295),
    shard INTEGER CHECK (shard IS NULL OR shard BETWEEN 0 AND 255),
    bits INTEGER CHECK (bits IS NULL OR bits BETWEEN 0 AND 4294967295),
    denc_version INTEGER CHECK (denc_version IS NULL OR denc_version BETWEEN 0 AND 255),
    decode_status TEXT NOT NULL CHECK (decode_status IN ('parsed', 'deferred')),
    deferred_reason TEXT CHECK (
        deferred_reason IS NULL
        OR (
            length(deferred_reason) BETWEEN 1 AND 128
            AND deferred_reason NOT GLOB '*[^0-9A-Za-z.:_-]*'
        )
    ),
    PRIMARY KEY (inventory_id, collection_identity),
    CHECK (
        (kind = 'meta' AND pool IS NULL AND seed IS NULL AND shard IS NULL)
        OR (kind IN ('head', 'temp') AND pool IS NOT NULL AND seed IS NOT NULL)
    ),
    CHECK (
        (decode_status = 'parsed' AND bits IS NOT NULL
            AND denc_version IS NOT NULL AND deferred_reason IS NULL)
        OR (decode_status = 'deferred' AND deferred_reason IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_collections_pg
ON ceph_bluestore_collections(inventory_id, pool, seed, shard, kind);

CREATE TABLE IF NOT EXISTS ceph_bluestore_objects (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    object_identity_sha256 TEXT NOT NULL CHECK (
        length(object_identity_sha256) = 64
        AND object_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    decoded_shard INTEGER NOT NULL CHECK (decoded_shard BETWEEN -1 AND 127),
    decoded_pool INTEGER NOT NULL,
    decoded_hash INTEGER NOT NULL CHECK (decoded_hash BETWEEN 0 AND 4294967295),
    decoded_bitwise_hash INTEGER NOT NULL
        CHECK (decoded_bitwise_hash BETWEEN 0 AND 4294967295),
    object_namespace BLOB NOT NULL,
    object_key BLOB,
    object_name BLOB NOT NULL,
    snap_hex TEXT NOT NULL CHECK (
        length(snap_hex) = 16 AND snap_hex NOT GLOB '*[^0-9a-f]*'
    ),
    generation_hex TEXT NOT NULL CHECK (
        length(generation_hex) = 16
        AND generation_hex NOT GLOB '*[^0-9a-f]*'
    ),
    onode_denc_version INTEGER NOT NULL CHECK (onode_denc_version BETWEEN 0 AND 255),
    nid INTEGER NOT NULL CHECK (nid >= 0),
    size INTEGER NOT NULL CHECK (size >= 0),
    flags_raw INTEGER NOT NULL CHECK (flags_raw BETWEEN 0 AND 255),
    flag_omap INTEGER NOT NULL CHECK (flag_omap IN (0, 1)),
    flag_pgmeta_omap INTEGER NOT NULL CHECK (flag_pgmeta_omap IN (0, 1)),
    flag_per_pool_omap INTEGER NOT NULL CHECK (flag_per_pool_omap IN (0, 1)),
    flag_per_pg_omap INTEGER NOT NULL CHECK (flag_per_pg_omap IN (0, 1)),
    flags_unknown_bits INTEGER NOT NULL CHECK (flags_unknown_bits BETWEEN 0 AND 255),
    attribute_count INTEGER NOT NULL CHECK (attribute_count >= 0),
    attribute_value_bytes INTEGER NOT NULL CHECK (attribute_value_bytes >= 0),
    attributes_sha256 TEXT NOT NULL CHECK (
        length(attributes_sha256) = 64
        AND attributes_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    expected_object_size INTEGER NOT NULL CHECK (expected_object_size >= 0),
    expected_write_size INTEGER NOT NULL CHECK (expected_write_size >= 0),
    allocation_hint_flags INTEGER NOT NULL
        CHECK (allocation_hint_flags BETWEEN 0 AND 4294967295),
    zone_ref_count INTEGER NOT NULL CHECK (zone_ref_count >= 0),
    extent_storage TEXT NOT NULL CHECK (
        extent_storage IN ('inline', 'sharded', 'deferred')
    ),
    spanning_blob_version INTEGER NOT NULL
        CHECK (spanning_blob_version BETWEEN 0 AND 255),
    declared_spanning_blob_count INTEGER NOT NULL
        CHECK (declared_spanning_blob_count >= 0),
    decode_status TEXT NOT NULL CHECK (decode_status IN ('parsed', 'deferred')),
    deferred_reason TEXT CHECK (
        deferred_reason IS NULL
        OR (
            length(deferred_reason) BETWEEN 1 AND 128
            AND deferred_reason NOT GLOB '*[^0-9A-Za-z.:_-]*'
        )
    ),
    onode_shard_count INTEGER NOT NULL CHECK (onode_shard_count >= 0),
    blob_count INTEGER NOT NULL CHECK (blob_count >= 0),
    logical_extent_count INTEGER NOT NULL CHECK (logical_extent_count >= 0),
    physical_extent_count INTEGER NOT NULL CHECK (physical_extent_count >= 0),
    PRIMARY KEY (inventory_id, object_identity_sha256),
    CHECK (
        (decode_status = 'parsed' AND deferred_reason IS NULL)
        OR (decode_status = 'deferred' AND deferred_reason IS NOT NULL)
    ),
    CHECK (
        (extent_storage = 'sharded' AND onode_shard_count > 0)
        OR (extent_storage <> 'sharded' AND onode_shard_count = 0)
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_objects_pool_hash
ON ceph_bluestore_objects(
    inventory_id,
    decoded_pool,
    decoded_hash,
    decoded_shard
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_shared_blobs (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_bluestore_semantic_scans(inventory_id) ON DELETE CASCADE,
    shared_blob_id_hex TEXT NOT NULL CHECK (
        length(shared_blob_id_hex) = 16
        AND shared_blob_id_hex NOT GLOB '*[^0-9a-f]*'
    ),
    denc_version INTEGER CHECK (denc_version IS NULL OR denc_version BETWEEN 0 AND 255),
    decode_status TEXT NOT NULL CHECK (decode_status IN ('parsed', 'deferred')),
    deferred_reason TEXT CHECK (
        deferred_reason IS NULL
        OR (
            length(deferred_reason) BETWEEN 1 AND 128
            AND deferred_reason NOT GLOB '*[^0-9A-Za-z.:_-]*'
        )
    ),
    ref_extent_count INTEGER NOT NULL CHECK (ref_extent_count >= 0),
    total_ref_bytes INTEGER NOT NULL CHECK (total_ref_bytes >= 0),
    total_refs INTEGER NOT NULL CHECK (total_refs >= 0),
    PRIMARY KEY (inventory_id, shared_blob_id_hex),
    CHECK (
        (decode_status = 'parsed' AND denc_version IS NOT NULL
            AND deferred_reason IS NULL)
        OR (decode_status = 'deferred' AND deferred_reason IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_onode_shards (
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL,
    shard_ordinal INTEGER NOT NULL CHECK (shard_ordinal >= 0),
    shard_offset INTEGER NOT NULL CHECK (shard_offset BETWEEN 0 AND 4294967295),
    descriptor_bytes INTEGER NOT NULL CHECK (descriptor_bytes > 0),
    payload_version INTEGER CHECK (
        payload_version IS NULL OR payload_version BETWEEN 0 AND 255
    ),
    declared_extent_count INTEGER CHECK (
        declared_extent_count IS NULL OR declared_extent_count >= 0
    ),
    payload_encoded_length INTEGER CHECK (
        payload_encoded_length IS NULL OR payload_encoded_length >= 0
    ),
    decode_status TEXT NOT NULL CHECK (decode_status IN ('parsed', 'deferred')),
    deferred_reason TEXT CHECK (
        deferred_reason IS NULL
        OR (
            length(deferred_reason) BETWEEN 1 AND 128
            AND deferred_reason NOT GLOB '*[^0-9A-Za-z.:_-]*'
        )
    ),
    logical_extent_count INTEGER NOT NULL CHECK (logical_extent_count >= 0),
    PRIMARY KEY (inventory_id, object_identity_sha256, shard_ordinal),
    UNIQUE (inventory_id, object_identity_sha256, shard_offset),
    FOREIGN KEY (inventory_id, object_identity_sha256)
        REFERENCES ceph_bluestore_objects(inventory_id, object_identity_sha256)
        ON DELETE CASCADE,
    CHECK (
        (decode_status = 'parsed' AND payload_version IS NOT NULL
            AND declared_extent_count IS NOT NULL
            AND payload_encoded_length IS NOT NULL
            AND deferred_reason IS NULL)
        OR (decode_status = 'deferred' AND deferred_reason IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_blobs (
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL,
    blob_ordinal INTEGER NOT NULL CHECK (blob_ordinal >= 0),
    blob_kind TEXT NOT NULL CHECK (blob_kind IN ('local', 'spanning')),
    blob_id_hex TEXT NOT NULL CHECK (
        length(blob_id_hex) = 16 AND blob_id_hex NOT GLOB '*[^0-9a-f]*'
    ),
    shared_blob_id_hex TEXT CHECK (
        shared_blob_id_hex IS NULL
        OR (
            length(shared_blob_id_hex) = 16
            AND shared_blob_id_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    logical_length INTEGER NOT NULL CHECK (logical_length >= 0),
    on_disk_length INTEGER NOT NULL CHECK (on_disk_length >= 0),
    compressed_length INTEGER CHECK (
        compressed_length IS NULL OR compressed_length > 0
    ),
    flags_raw INTEGER NOT NULL CHECK (flags_raw BETWEEN 0 AND 4294967295),
    flag_legacy_mutable INTEGER NOT NULL CHECK (flag_legacy_mutable IN (0, 1)),
    flag_compressed INTEGER NOT NULL CHECK (flag_compressed IN (0, 1)),
    flag_checksum INTEGER NOT NULL CHECK (flag_checksum IN (0, 1)),
    flag_has_unused INTEGER NOT NULL CHECK (flag_has_unused IN (0, 1)),
    flag_shared INTEGER NOT NULL CHECK (flag_shared IN (0, 1)),
    flags_unknown_bits INTEGER NOT NULL
        CHECK (flags_unknown_bits BETWEEN 0 AND 4294967295),
    unused_bitmap INTEGER CHECK (
        unused_bitmap IS NULL OR unused_bitmap BETWEEN 0 AND 65535
    ),
    checksum_type TEXT CHECK (
        checksum_type IS NULL
        OR (length(checksum_type) > 0 AND instr(checksum_type, char(0)) = 0)
    ),
    checksum_order INTEGER CHECK (
        checksum_order IS NULL OR checksum_order BETWEEN 0 AND 255
    ),
    checksum_chunk_size INTEGER CHECK (
        checksum_chunk_size IS NULL OR checksum_chunk_size > 0
    ),
    checksum_encoded_length INTEGER CHECK (
        checksum_encoded_length IS NULL OR checksum_encoded_length > 0
    ),
    checksum_value_count INTEGER NOT NULL CHECK (checksum_value_count >= 0),
    checksum_data_crc32c INTEGER CHECK (
        checksum_data_crc32c IS NULL
        OR checksum_data_crc32c BETWEEN 0 AND 4294967295
    ),
    checksum_digest_sha256 TEXT CHECK (
        checksum_digest_sha256 IS NULL
        OR (
            length(checksum_digest_sha256) = 64
            AND checksum_digest_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    use_tracker_kind TEXT CHECK (
        use_tracker_kind IS NULL
        OR use_tracker_kind IN ('v1LegacyRefMap', 'v2')
    ),
    use_tracker_allocation_unit_size INTEGER CHECK (
        use_tracker_allocation_unit_size IS NULL
        OR use_tracker_allocation_unit_size >= 0
    ),
    use_tracker_declared_allocation_units INTEGER CHECK (
        use_tracker_declared_allocation_units IS NULL
        OR use_tracker_declared_allocation_units >= 0
    ),
    use_tracker_entry_count INTEGER NOT NULL CHECK (use_tracker_entry_count >= 0),
    use_tracker_sha256 TEXT CHECK (
        use_tracker_sha256 IS NULL
        OR (
            length(use_tracker_sha256) = 64
            AND use_tracker_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    logical_extent_count INTEGER NOT NULL CHECK (logical_extent_count >= 0),
    physical_extent_count INTEGER NOT NULL CHECK (physical_extent_count >= 0),
    PRIMARY KEY (inventory_id, object_identity_sha256, blob_ordinal),
    UNIQUE (inventory_id, object_identity_sha256, blob_kind, blob_id_hex),
    FOREIGN KEY (inventory_id, object_identity_sha256)
        REFERENCES ceph_bluestore_objects(inventory_id, object_identity_sha256)
        ON DELETE CASCADE,
    FOREIGN KEY (inventory_id, shared_blob_id_hex)
        REFERENCES ceph_bluestore_shared_blobs(inventory_id, shared_blob_id_hex)
        ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED,
    CHECK (
        (flag_compressed = 0 AND compressed_length IS NULL
            AND logical_length = on_disk_length)
        OR (flag_compressed = 1 AND compressed_length IS NOT NULL
            AND logical_length > 0
            AND compressed_length <= on_disk_length)
    ),
    CHECK (flag_checksum = ((flags_raw & 4) <> 0)),
    CHECK (flag_has_unused = ((flags_raw & 8) <> 0)),
    CHECK (flag_shared = ((flags_raw & 16) <> 0)),
    CHECK (flag_legacy_mutable = ((flags_raw & 1) <> 0)),
    CHECK (flag_compressed = ((flags_raw & 2) <> 0)),
    CHECK (flags_unknown_bits = (flags_raw & ~31)),
    CHECK (flag_has_unused = (unused_bitmap IS NOT NULL)),
    CHECK (flag_shared = (shared_blob_id_hex IS NOT NULL)),
    CHECK (
        (flag_checksum = 0 AND checksum_value_count = 0 AND checksum_type IS NULL
            AND checksum_order IS NULL AND checksum_chunk_size IS NULL
            AND checksum_encoded_length IS NULL
            AND checksum_data_crc32c IS NULL
            AND checksum_digest_sha256 IS NULL)
        OR (flag_checksum = 1 AND checksum_value_count > 0
            AND checksum_type IS NOT NULL
            AND checksum_order IS NOT NULL AND checksum_chunk_size IS NOT NULL
            AND checksum_encoded_length IS NOT NULL
            AND checksum_data_crc32c IS NOT NULL
            AND checksum_digest_sha256 IS NOT NULL)
    ),
    CHECK (
        (use_tracker_kind IS NULL AND use_tracker_allocation_unit_size IS NULL
            AND use_tracker_declared_allocation_units IS NULL
            AND use_tracker_entry_count = 0 AND use_tracker_sha256 IS NULL)
        OR (use_tracker_kind = 'v1LegacyRefMap'
            AND use_tracker_allocation_unit_size IS NULL
            AND use_tracker_declared_allocation_units IS NULL
            AND use_tracker_sha256 IS NOT NULL)
        OR (use_tracker_kind = 'v2'
            AND use_tracker_allocation_unit_size IS NOT NULL
            AND use_tracker_declared_allocation_units IS NOT NULL
            AND use_tracker_sha256 IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_logical_extents (
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL,
    extent_ordinal INTEGER NOT NULL CHECK (extent_ordinal >= 0),
    logical_offset INTEGER NOT NULL CHECK (logical_offset >= 0),
    length INTEGER NOT NULL CHECK (length > 0),
    blob_ordinal INTEGER NOT NULL CHECK (blob_ordinal >= 0),
    blob_offset INTEGER NOT NULL CHECK (blob_offset >= 0),
    shard_ordinal INTEGER CHECK (shard_ordinal IS NULL OR shard_ordinal >= 0),
    defines_blob INTEGER NOT NULL CHECK (defines_blob IN (0, 1)),
    flags_raw INTEGER NOT NULL CHECK (flags_raw BETWEEN 0 AND 15),
    flag_contiguous INTEGER NOT NULL CHECK (flag_contiguous IN (0, 1)),
    flag_zero_blob_offset INTEGER NOT NULL CHECK (flag_zero_blob_offset IN (0, 1)),
    flag_same_length INTEGER NOT NULL CHECK (flag_same_length IN (0, 1)),
    flag_spanning INTEGER NOT NULL CHECK (flag_spanning IN (0, 1)),
    PRIMARY KEY (inventory_id, object_identity_sha256, extent_ordinal),
    UNIQUE (inventory_id, object_identity_sha256, logical_offset),
    FOREIGN KEY (inventory_id, object_identity_sha256, blob_ordinal)
        REFERENCES ceph_bluestore_blobs(
            inventory_id,
            object_identity_sha256,
            blob_ordinal
        )
        ON DELETE CASCADE,
    FOREIGN KEY (inventory_id, object_identity_sha256, shard_ordinal)
        REFERENCES ceph_bluestore_onode_shards(
            inventory_id,
            object_identity_sha256,
            shard_ordinal
        )
        ON DELETE CASCADE,
    CHECK (flag_contiguous = ((flags_raw & 1) <> 0)),
    CHECK (flag_zero_blob_offset = ((flags_raw & 2) <> 0)),
    CHECK (flag_same_length = ((flags_raw & 4) <> 0)),
    CHECK (flag_spanning = ((flags_raw & 8) <> 0))
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_logical_extents_range
ON ceph_bluestore_logical_extents(
    inventory_id,
    object_identity_sha256,
    logical_offset
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_physical_extents (
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL,
    blob_ordinal INTEGER NOT NULL CHECK (blob_ordinal >= 0),
    extent_ordinal INTEGER NOT NULL CHECK (extent_ordinal >= 0),
    blob_offset INTEGER NOT NULL CHECK (blob_offset >= 0),
    device_id INTEGER NOT NULL CHECK (device_id BETWEEN 0 AND 255),
    physical_offset_hex TEXT CHECK (
        physical_offset_hex IS NULL
        OR (
            length(physical_offset_hex) = 16
            AND physical_offset_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    length INTEGER NOT NULL CHECK (length > 0),
    PRIMARY KEY (
        inventory_id,
        object_identity_sha256,
        blob_ordinal,
        extent_ordinal
    ),
    UNIQUE (
        inventory_id,
        object_identity_sha256,
        blob_ordinal,
        blob_offset
    ),
    FOREIGN KEY (inventory_id, object_identity_sha256, blob_ordinal)
        REFERENCES ceph_bluestore_blobs(
            inventory_id,
            object_identity_sha256,
            blob_ordinal
        )
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_physical_extents_device
ON ceph_bluestore_physical_extents(
    inventory_id,
    device_id,
    physical_offset_hex
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_checksum_chunks (
    inventory_id TEXT NOT NULL,
    object_identity_sha256 TEXT NOT NULL,
    blob_ordinal INTEGER NOT NULL CHECK (blob_ordinal >= 0),
    checksum_ordinal INTEGER NOT NULL CHECK (checksum_ordinal >= 0),
    chunk_offset INTEGER NOT NULL CHECK (chunk_offset >= 0),
    chunk_length INTEGER NOT NULL CHECK (chunk_length > 0),
    checksum_value_hex TEXT NOT NULL CHECK (
        length(checksum_value_hex) BETWEEN 2 AND 128
        AND length(checksum_value_hex) % 2 = 0
        AND checksum_value_hex NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (
        inventory_id,
        object_identity_sha256,
        blob_ordinal,
        checksum_ordinal
    ),
    UNIQUE (
        inventory_id,
        object_identity_sha256,
        blob_ordinal,
        chunk_offset
    ),
    FOREIGN KEY (inventory_id, object_identity_sha256, blob_ordinal)
        REFERENCES ceph_bluestore_blobs(
            inventory_id,
            object_identity_sha256,
            blob_ordinal
        )
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ceph_bluestore_shared_blob_refs (
    inventory_id TEXT NOT NULL,
    shared_blob_id_hex TEXT NOT NULL,
    ref_ordinal INTEGER NOT NULL CHECK (ref_ordinal >= 0),
    ref_offset_hex TEXT NOT NULL CHECK (
        length(ref_offset_hex) = 16
        AND ref_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    length INTEGER NOT NULL CHECK (length > 0),
    refs INTEGER NOT NULL CHECK (refs > 0),
    PRIMARY KEY (inventory_id, shared_blob_id_hex, ref_ordinal),
    UNIQUE (inventory_id, shared_blob_id_hex, ref_offset_hex),
    FOREIGN KEY (inventory_id, shared_blob_id_hex)
        REFERENCES ceph_bluestore_shared_blobs(inventory_id, shared_blob_id_hex)
        ON DELETE CASCADE
);
