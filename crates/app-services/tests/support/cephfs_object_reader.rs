use std::path::Path;

use app_services::ceph_reconstruction::{
    inventory_cephfs_metadata_pool, CephFsDescriptor, CephFsDescriptorState, CephFsObjectLocator,
    CephFsObjectSource, CephFsPoolBinding, CephFsPoolProvenance, CephFsPoolRole,
};
use domain::DataSourceId;
use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::{
        latest_state_set_sha256, object_identity_sha256, semantic_aggregate_sha256,
        CephBluestoreBlobRecord, CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord,
        CephBluestorePhysicalExtentRecord, CephBluestoreSemanticAggregate,
        CephBluestoreSemanticRepo, CephBluestoreSemanticScanRecord, CephBluestoreSuperRecord,
        BLUESTORE_SEMANTIC_DECODE_PROFILE, BLUESTORE_SEMANTIC_SCHEMA_VERSION,
    },
    ceph_rocksdb_latest_state_repo::{CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo},
};
use rusqlite::{params, Connection};

pub const DEVICE_SIZE: usize = 1024 * 1024;
pub const PHYSICAL_OFFSET: usize = 4096;
pub const OBJECT_NAME: &[u8] = b"1.00000000";
pub const OBJECT_SIZE: u64 = 16;
const FILESYSTEM_ID: i64 = 1;
const FSMAP_EPOCH: u32 = 17;
const METADATA_POOL: i64 = 7;
pub const DATA_POOL: i64 = 8;

pub fn descriptor(bindings: &[(&str, &str)]) -> CephFsDescriptor {
    CephFsDescriptor {
        identity: "ceph-fs:cluster-a:1:17:7".to_string(),
        cluster_identity: "cluster-a".to_string(),
        filesystem_id: FILESYSTEM_ID,
        name: "cephfs-a".to_string(),
        fsmap_epoch: FSMAP_EPOCH,
        mdsmap_epoch: FSMAP_EPOCH,
        state: CephFsDescriptorState::Present,
        metadata_pool: CephFsPoolBinding {
            pool_id: METADATA_POOL,
            role: CephFsPoolRole::Metadata,
            provenance: bindings
                .iter()
                .map(|(source, inventory)| CephFsPoolProvenance {
                    source_identity: (*source).to_string(),
                    inventory_identity: (*inventory).to_string(),
                })
                .collect(),
        },
        data_pools: Vec::new(),
        rank_bindings: Vec::new(),
        daemons: Vec::new(),
        provenance: Vec::new(),
    }
}

pub fn descriptor_with_data_pool(bindings: &[(&str, &str)]) -> CephFsDescriptor {
    let mut descriptor = descriptor(bindings);
    descriptor.data_pools.push(CephFsPoolBinding {
        pool_id: DATA_POOL,
        role: CephFsPoolRole::Data { ordinal: 0 },
        provenance: bindings
            .iter()
            .map(|(source, inventory)| CephFsPoolProvenance {
                source_identity: (*source).to_string(),
                inventory_identity: (*inventory).to_string(),
            })
            .collect(),
    });
    descriptor
}

pub fn locator() -> CephFsObjectLocator {
    CephFsObjectLocator::new(
        FILESYSTEM_ID,
        METADATA_POOL,
        Vec::new(),
        OBJECT_NAME.to_vec(),
        FSMAP_EPOCH,
    )
    .expect("build CephFS locator")
}

pub fn data_locator() -> CephFsObjectLocator {
    CephFsObjectLocator::new(
        FILESYSTEM_ID,
        DATA_POOL,
        Vec::new(),
        OBJECT_NAME.to_vec(),
        FSMAP_EPOCH,
    )
    .expect("build CephFS data locator")
}

pub fn write_source(
    path: &Path,
    descriptor: &CephFsDescriptor,
    source: &str,
    inventory: &str,
    object_size: Option<u64>,
) -> CephFsObjectSource {
    let conn = persistence_sqlite::open_or_create_source(path).expect("open source database");
    seed_control_plane(&conn, source, inventory);
    let aggregate = semantic_aggregate(inventory, object_size);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&aggregate)
        .expect("persist semantic aggregate");
    inventory_cephfs_metadata_pool(&conn, descriptor, source, inventory)
        .expect("persist CephFS metadata inventory");
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint source database");
    drop(conn);
    CephFsObjectSource::new(
        DataSourceId(source.to_string()),
        inventory,
        path.to_path_buf(),
    )
    .expect("build CephFS object source")
}

pub fn write_data_source(
    path: &Path,
    source: &str,
    inventory: &str,
    object_size: Option<u64>,
) -> CephFsObjectSource {
    let conn = persistence_sqlite::open_or_create_source(path).expect("open source database");
    seed_control_plane(&conn, source, inventory);
    let aggregate = semantic_aggregate_for(inventory, object_size, DATA_POOL, OBJECT_NAME);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&aggregate)
        .expect("persist data-pool semantic aggregate");
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint source database");
    drop(conn);
    CephFsObjectSource::new(
        DataSourceId(source.to_string()),
        inventory,
        path.to_path_buf(),
    )
    .expect("build CephFS data object source")
}

fn seed_control_plane(conn: &Connection, source: &str, inventory: &str) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-19T00:00:00Z')",
        [source],
    )
    .expect("insert data source");
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (?1, ?2, ?1, 'block', ?3, 1, 0, 'BlueStore OSD', 1,
                   1, 'singleReplica', 1, '{}')",
        params![inventory, source, DEVICE_SIZE as u64],
    )
    .expect("insert OSD inventory");
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
                   4096, 1, 0, 0)",
        [inventory, source],
    )
    .expect("insert BlueFS superblock");
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [inventory],
    )
    .expect("insert BlueFS replay");
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
                   'leveldb.BytewiseComparator', 100, 150, 142, 0, 0)",
        [inventory, source],
    )
    .expect("insert RocksDB manifest");
    conn.execute(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped, log_number
         ) VALUES (?1, 0, 'default', 'leveldb.BytewiseComparator', 0, 142)",
        [inventory],
    )
    .expect("insert RocksDB column family");
    CephRocksdbLatestStateRepo::new(conn)
        .replace_for_inventory(inventory, &latest_state(inventory))
        .expect("persist RocksDB latest state");
}

fn semantic_aggregate(inventory: &str, object_size: Option<u64>) -> CephBluestoreSemanticAggregate {
    semantic_aggregate_for(inventory, object_size, METADATA_POOL, OBJECT_NAME)
}

fn semantic_aggregate_for(
    inventory: &str,
    object_size: Option<u64>,
    pool: i64,
    object_name: &[u8],
) -> CephBluestoreSemanticAggregate {
    let latest_state = latest_state(inventory);
    let mut objects = Vec::new();
    let mut blobs = Vec::new();
    let mut logical_extents = Vec::new();
    let mut physical_extents = Vec::new();
    if let Some(size) = object_size {
        let object = object_record(inventory, size, pool, object_name);
        let identity = object.object_identity_sha256.clone();
        objects.push(object);
        blobs.push(blob_record(inventory, &identity, size));
        logical_extents.push(logical_extent(inventory, &identity, size));
        physical_extents.push(physical_extent(inventory, &identity, size));
    }
    let object_count = objects.len() as u64;
    let mut aggregate = CephBluestoreSemanticAggregate {
        scan: CephBluestoreSemanticScanRecord {
            inventory_id: inventory.to_string(),
            schema_version: BLUESTORE_SEMANTIC_SCHEMA_VERSION,
            decode_profile: BLUESTORE_SEMANTIC_DECODE_PROFILE.to_string(),
            sharding_sha256: "a".repeat(64),
            latest_state_sha256: latest_state_set_sha256(&latest_state),
            semantic_sha256: "0".repeat(64),
            s_latest_count: 7,
            s_decoded_count: 7,
            s_deferred_count: 0,
            c_latest_count: 0,
            c_decoded_count: 0,
            c_deferred_count: 0,
            o_latest_count: object_count,
            o_decoded_count: object_count,
            o_deferred_count: 0,
            x_latest_count: 0,
            x_decoded_count: 0,
            x_deferred_count: 0,
            collection_count: 0,
            object_count,
            blob_count: object_count,
            onode_shard_count: 0,
            logical_extent_count: object_count,
            physical_extent_count: object_count,
            checksum_chunk_count: 0,
            shared_blob_count: 0,
            shared_ref_extent_count: 0,
            profile_complete: true,
        },
        super_record: CephBluestoreSuperRecord {
            inventory_id: inventory.to_string(),
            nid_max: Some(100),
            blobid_max: Some(200),
            min_alloc_size: Some(4096),
            ondisk_format: Some(4),
            min_compat_ondisk_format: Some(3),
            per_pool_omap: Some("perPg".to_string()),
            freelist_type: Some("bitmap".to_string()),
            observed_count: 7,
            deferred_count: 0,
        },
        collections: Vec::new(),
        objects,
        onode_shards: Vec::new(),
        blobs,
        logical_extents,
        physical_extents,
        checksum_chunks: Vec::new(),
        shared_blobs: Vec::new(),
        shared_blob_refs: Vec::new(),
    };
    aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(&aggregate);
    aggregate
}

fn object_record(
    inventory: &str,
    size: u64,
    pool: i64,
    object_name: &[u8],
) -> CephBluestoreObjectRecord {
    let mut object = CephBluestoreObjectRecord {
        inventory_id: inventory.to_string(),
        object_identity_sha256: String::new(),
        decoded_shard: -1,
        decoded_pool: pool,
        decoded_hash: 1,
        decoded_bitwise_hash: 2_147_483_648,
        object_namespace: Vec::new(),
        object_key: None,
        object_name: object_name.to_vec(),
        snap_hex: "fffffffffffffffe".to_string(),
        generation_hex: "0000000000000000".to_string(),
        onode_denc_version: 1,
        nid: 1,
        size,
        flags_raw: 0,
        flag_omap: false,
        flag_pgmeta_omap: false,
        flag_per_pool_omap: false,
        flag_per_pg_omap: false,
        flags_unknown_bits: 0,
        attribute_count: 0,
        attribute_value_bytes: 0,
        attributes_sha256: "e".repeat(64),
        expected_object_size: size,
        expected_write_size: size,
        allocation_hint_flags: 0,
        zone_ref_count: 0,
        extent_storage: "inline".to_string(),
        spanning_blob_version: 1,
        declared_spanning_blob_count: 0,
        decode_status: "parsed".to_string(),
        deferred_reason: None,
        onode_shard_count: 0,
        blob_count: 1,
        logical_extent_count: 1,
        physical_extent_count: 1,
    };
    object.object_identity_sha256 = object_identity_sha256(&object);
    object
}

fn blob_record(inventory: &str, identity: &str, size: u64) -> CephBluestoreBlobRecord {
    CephBluestoreBlobRecord {
        inventory_id: inventory.to_string(),
        object_identity_sha256: identity.to_string(),
        blob_ordinal: 0,
        blob_kind: "local".to_string(),
        blob_id_hex: "0000000000000001".to_string(),
        shared_blob_id_hex: None,
        logical_length: size,
        on_disk_length: size,
        compressed_length: None,
        flags_raw: 0,
        flag_legacy_mutable: false,
        flag_compressed: false,
        flag_checksum: false,
        flag_has_unused: false,
        flag_shared: false,
        flags_unknown_bits: 0,
        unused_bitmap: None,
        checksum_type: None,
        checksum_order: None,
        checksum_chunk_size: None,
        checksum_encoded_length: None,
        checksum_value_count: 0,
        checksum_data_crc32c: None,
        checksum_digest_sha256: None,
        use_tracker_kind: None,
        use_tracker_allocation_unit_size: None,
        use_tracker_declared_allocation_units: None,
        use_tracker_entry_count: 0,
        use_tracker_sha256: None,
        logical_extent_count: 1,
        physical_extent_count: 1,
    }
}

fn logical_extent(inventory: &str, identity: &str, size: u64) -> CephBluestoreLogicalExtentRecord {
    CephBluestoreLogicalExtentRecord {
        inventory_id: inventory.to_string(),
        object_identity_sha256: identity.to_string(),
        extent_ordinal: 0,
        logical_offset: 0,
        length: size,
        blob_ordinal: 0,
        blob_offset: 0,
        shard_ordinal: None,
        defines_blob: true,
        flags_raw: 6,
        flag_contiguous: false,
        flag_zero_blob_offset: true,
        flag_same_length: true,
        flag_spanning: false,
    }
}

fn physical_extent(
    inventory: &str,
    identity: &str,
    size: u64,
) -> CephBluestorePhysicalExtentRecord {
    CephBluestorePhysicalExtentRecord {
        inventory_id: inventory.to_string(),
        object_identity_sha256: identity.to_string(),
        blob_ordinal: 0,
        extent_ordinal: 0,
        blob_offset: 0,
        device_id: 1,
        physical_offset_hex: Some(format!("{PHYSICAL_OFFSET:016x}")),
        length: size,
    }
}

fn latest_state(inventory: &str) -> Vec<CephRocksdbLatestStateRecord> {
    vec![CephRocksdbLatestStateRecord {
        inventory_id: inventory.to_string(),
        column_family_id: 0,
        column_family_name: "default".to_string(),
        schema_version: 1,
        sharding_sha256: "a".repeat(64),
        point_mutation_count: 0,
        sst_point_mutation_count: 0,
        wal_point_mutation_count: 0,
        range_mutation_count: 0,
        sst_range_mutation_count: 0,
        wal_range_mutation_count: 0,
        latest_value_count: 0,
        deleted_key_count: 0,
        delete_decision_count: 0,
        single_delete_decision_count: 0,
        range_delete_decision_count: 0,
        merge_resolved_count: 0,
        merge_operand_count: 0,
        range_hidden_version_count: 0,
        smallest_sequence: None,
        largest_sequence: None,
        point_sha256: "b".repeat(64),
        range_sha256: "c".repeat(64),
        latest_state_sha256: "d".repeat(64),
        scan_complete: true,
    }]
}
