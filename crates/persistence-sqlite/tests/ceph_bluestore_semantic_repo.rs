use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_bluestore_semantic_repo::{
        canonical_collection_identity, latest_state_set_sha256, object_identity_sha256,
        semantic_aggregate_sha256, validate_replacement, CephBluestoreBlobRecord,
        CephBluestoreChecksumChunkRecord, CephBluestoreCollectionRecord,
        CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
        CephBluestorePhysicalExtentRecord, CephBluestoreReadPlanSession,
        CephBluestoreSemanticAggregate, CephBluestoreSemanticRepo, CephBluestoreSemanticScanRecord,
        CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
    },
    repositories::ceph_rocksdb_latest_state_repo::{
        CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo,
    },
    runner,
};
use rusqlite::{params, Connection};

const INVENTORY_A: &str = "inventory-a";
const INVENTORY_B: &str = "inventory-b";
const RBD_HEAD_SNAP_HEX: &str = "fffffffffffffffe";

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    seed_control_plane(&conn, INVENTORY_A, "source-a");
    seed_control_plane(&conn, INVENTORY_B, "source-b");
    conn
}

fn seed_control_plane(conn: &Connection, inventory_id: &str, data_source_id: &str) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-15T00:00:00Z')",
        [data_source_id],
    )
    .expect("insert data source");
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (
            ?1, ?2, ?1, 'block', 1048576, 1, 0, 'BlueStore OSD', 1,
            1, 'singleReplica', 1, '{}'
         )",
        [inventory_id, data_source_id],
    )
    .expect("insert OSD");
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (
            ?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
            4096, 1, 0, 0
         )",
        [inventory_id, data_source_id],
    )
    .expect("insert BlueFS superblock");
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [inventory_id],
    )
    .expect("insert BlueFS replay");
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (
            ?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
            'leveldb.BytewiseComparator', 100, 150, 142, 0, 0
         )",
        [inventory_id, data_source_id],
    )
    .expect("insert RocksDB manifest");
    conn.execute(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped, log_number
         ) VALUES (?1, 0, 'default', 'leveldb.BytewiseComparator', 0, 142)",
        [inventory_id],
    )
    .expect("insert active column family");
    CephRocksdbLatestStateRepo::new(conn)
        .replace_for_inventory(inventory_id, &latest_state(inventory_id))
        .expect("insert latest state");
}

fn aggregate(inventory_id: &str) -> CephBluestoreSemanticAggregate {
    let latest_state = latest_state(inventory_id);
    let mut object = object(inventory_id);
    object.object_identity_sha256 = object_identity_sha256(&object);
    let object_id = object.object_identity_sha256.clone();
    let collections = vec![
        collection(inventory_id, "meta", None, None, None, 7),
        collection(inventory_id, "head", Some(7), Some(0x1a), None, 8),
    ];
    let mut aggregate = CephBluestoreSemanticAggregate {
        scan: CephBluestoreSemanticScanRecord {
            inventory_id: inventory_id.to_string(),
            schema_version: 1,
            decode_profile: "scox-v1".to_string(),
            sharding_sha256: "a".repeat(64),
            latest_state_sha256: latest_state_set_sha256(&latest_state),
            semantic_sha256: "c".repeat(64),
            s_latest_count: 7,
            s_decoded_count: 7,
            s_deferred_count: 0,
            c_latest_count: 2,
            c_decoded_count: 2,
            c_deferred_count: 0,
            o_latest_count: 2,
            o_decoded_count: 2,
            o_deferred_count: 0,
            x_latest_count: 1,
            x_decoded_count: 1,
            x_deferred_count: 0,
            collection_count: 2,
            object_count: 1,
            blob_count: 1,
            onode_shard_count: 1,
            logical_extent_count: 2,
            physical_extent_count: 1,
            checksum_chunk_count: 1,
            shared_blob_count: 1,
            shared_ref_extent_count: 1,
            profile_complete: true,
        },
        super_record: CephBluestoreSuperRecord {
            inventory_id: inventory_id.to_string(),
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
        collections,
        objects: vec![object],
        onode_shards: vec![CephBluestoreOnodeShardRecord {
            inventory_id: inventory_id.to_string(),
            object_identity_sha256: object_id.clone(),
            shard_ordinal: 0,
            shard_offset: 0,
            descriptor_bytes: 32,
            payload_version: Some(2),
            declared_extent_count: Some(2),
            payload_encoded_length: Some(32),
            decode_status: "parsed".to_string(),
            deferred_reason: None,
            logical_extent_count: 2,
        }],
        blobs: vec![CephBluestoreBlobRecord {
            inventory_id: inventory_id.to_string(),
            object_identity_sha256: object_id.clone(),
            blob_ordinal: 0,
            blob_kind: "local".to_string(),
            blob_id_hex: "1122334455667788".to_string(),
            shared_blob_id_hex: Some("8877665544332211".to_string()),
            logical_length: 4096,
            on_disk_length: 4096,
            compressed_length: None,
            flags_raw: 20,
            flag_legacy_mutable: false,
            flag_compressed: false,
            flag_checksum: true,
            flag_has_unused: false,
            flag_shared: true,
            flags_unknown_bits: 0,
            unused_bitmap: None,
            checksum_type: Some("crc32c".to_string()),
            checksum_order: Some(12),
            checksum_chunk_size: Some(4096),
            checksum_encoded_length: Some(4),
            checksum_value_count: 1,
            checksum_data_crc32c: Some(0x1234_5678),
            checksum_digest_sha256: Some("e".repeat(64)),
            use_tracker_kind: Some("v1LegacyRefMap".to_string()),
            use_tracker_allocation_unit_size: None,
            use_tracker_declared_allocation_units: None,
            use_tracker_entry_count: 1,
            use_tracker_sha256: Some("f".repeat(64)),
            logical_extent_count: 2,
            physical_extent_count: 1,
        }],
        logical_extents: vec![
            logical_extent(inventory_id, &object_id, 0, 0, 0),
            logical_extent(inventory_id, &object_id, 1, 2048, 2048),
        ],
        physical_extents: vec![CephBluestorePhysicalExtentRecord {
            inventory_id: inventory_id.to_string(),
            object_identity_sha256: object_id.clone(),
            blob_ordinal: 0,
            extent_ordinal: 0,
            blob_offset: 0,
            device_id: 1,
            physical_offset_hex: Some("0000000000001000".to_string()),
            length: 4096,
        }],
        checksum_chunks: vec![CephBluestoreChecksumChunkRecord {
            object_ordinal: 0,
            blob_ordinal: 0,
            checksum_ordinal: 0,
            chunk_offset: 0,
            chunk_length: 4096,
            checksum_value: 0x1234_5678,
            checksum_value_bytes: 4,
        }],
        shared_blobs: vec![CephBluestoreSharedBlobRecord {
            inventory_id: inventory_id.to_string(),
            shared_blob_id_hex: "8877665544332211".to_string(),
            denc_version: Some(1),
            decode_status: "parsed".to_string(),
            deferred_reason: None,
            ref_extent_count: 1,
            total_ref_bytes: 4096,
            total_refs: 1,
        }],
        shared_blob_refs: vec![CephBluestoreSharedBlobRefRecord {
            inventory_id: inventory_id.to_string(),
            shared_blob_id_hex: "8877665544332211".to_string(),
            ref_ordinal: 0,
            ref_offset_hex: "0000000000001000".to_string(),
            length: 4096,
            refs: 1,
        }],
    };
    aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(&aggregate);
    aggregate
}

fn latest_state(inventory_id: &str) -> Vec<CephRocksdbLatestStateRecord> {
    vec![CephRocksdbLatestStateRecord {
        inventory_id: inventory_id.to_string(),
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

fn collection(
    inventory_id: &str,
    kind: &str,
    pool: Option<u64>,
    seed: Option<u32>,
    shard: Option<u8>,
    bits: u32,
) -> CephBluestoreCollectionRecord {
    CephBluestoreCollectionRecord {
        inventory_id: inventory_id.to_string(),
        collection_identity: canonical_collection_identity(kind, pool, seed, shard)
            .expect("canonical collection"),
        kind: kind.to_string(),
        pool,
        seed,
        shard,
        bits: Some(bits),
        denc_version: Some(1),
        decode_status: "parsed".to_string(),
        deferred_reason: None,
    }
}

fn object(inventory_id: &str) -> CephBluestoreObjectRecord {
    CephBluestoreObjectRecord {
        inventory_id: inventory_id.to_string(),
        object_identity_sha256: String::new(),
        decoded_shard: -1,
        decoded_pool: 7,
        decoded_hash: 0x1234_5678,
        decoded_bitwise_hash: 0x1e6a_2c48,
        object_namespace: b"ns\0".to_vec(),
        object_key: Some(b"key".to_vec()),
        object_name: b"object".to_vec(),
        snap_hex: "000000000000000c".to_string(),
        generation_hex: "0000000000000022".to_string(),
        onode_denc_version: 2,
        nid: 17,
        size: 4096,
        flags_raw: 0x0d,
        flag_omap: true,
        flag_pgmeta_omap: false,
        flag_per_pool_omap: true,
        flag_per_pg_omap: true,
        flags_unknown_bits: 0,
        attribute_count: 2,
        attribute_value_bytes: 8,
        attributes_sha256: "d".repeat(64),
        expected_object_size: 8192,
        expected_write_size: 4096,
        allocation_hint_flags: 5,
        zone_ref_count: 1,
        extent_storage: "sharded".to_string(),
        spanning_blob_version: 2,
        declared_spanning_blob_count: 0,
        decode_status: "parsed".to_string(),
        deferred_reason: None,
        onode_shard_count: 1,
        blob_count: 1,
        logical_extent_count: 2,
        physical_extent_count: 1,
    }
}

fn logical_extent(
    inventory_id: &str,
    object_id: &str,
    ordinal: u32,
    logical_offset: u64,
    blob_offset: u64,
) -> CephBluestoreLogicalExtentRecord {
    let flag_contiguous = ordinal > 0;
    let flag_zero_blob_offset = blob_offset == 0;
    CephBluestoreLogicalExtentRecord {
        inventory_id: inventory_id.to_string(),
        object_identity_sha256: object_id.to_string(),
        extent_ordinal: ordinal,
        logical_offset,
        length: 2048,
        blob_ordinal: 0,
        blob_offset,
        shard_ordinal: Some(0),
        defines_blob: ordinal == 0,
        flags_raw: u8::from(flag_contiguous) | (u8::from(flag_zero_blob_offset) << 1),
        flag_contiguous,
        flag_zero_blob_offset,
        flag_same_length: false,
        flag_spanning: false,
    }
}

fn append_second_object(
    aggregate: &mut CephBluestoreSemanticAggregate,
    physical_offset_hex: &str,
    shared: bool,
) {
    let first_object_id = aggregate.objects[0].object_identity_sha256.clone();
    let mut object = aggregate.objects[0].clone();
    object.object_name = b"second-object".to_vec();
    object.object_identity_sha256 = object_identity_sha256(&object);
    let object_id = object.object_identity_sha256.clone();

    let mut shard = aggregate.onode_shards[0].clone();
    shard.object_identity_sha256 = object_id.clone();
    let mut blob = aggregate.blobs[0].clone();
    blob.object_identity_sha256 = object_id.clone();
    if !shared {
        blob.flags_raw &= !16;
        blob.flag_shared = false;
        blob.shared_blob_id_hex = None;
    }
    let mut logical = aggregate.logical_extents.clone();
    for extent in &mut logical {
        extent.object_identity_sha256 = object_id.clone();
    }
    let mut physical = aggregate.physical_extents[0].clone();
    physical.object_identity_sha256 = object_id.clone();
    physical.physical_offset_hex = Some(physical_offset_hex.to_string());
    let mut checksum = aggregate.checksum_chunks[0].clone();
    checksum.object_ordinal = 1;

    aggregate.objects.push(object);
    aggregate.onode_shards.push(shard);
    aggregate.blobs.push(blob);
    aggregate.logical_extents.extend(logical);
    aggregate.physical_extents.push(physical);
    aggregate.checksum_chunks.push(checksum);
    aggregate.objects.sort_by(|left, right| {
        left.object_identity_sha256
            .cmp(&right.object_identity_sha256)
    });
    let first_ordinal = aggregate
        .objects
        .iter()
        .position(|record| record.object_identity_sha256 == first_object_id)
        .expect("first object remains present") as u32;
    let second_ordinal = aggregate
        .objects
        .iter()
        .position(|record| record.object_identity_sha256 == object_id)
        .expect("second object remains present") as u32;
    aggregate.checksum_chunks[0].object_ordinal = first_ordinal;
    aggregate.checksum_chunks[1].object_ordinal = second_ordinal;
    aggregate
        .onode_shards
        .sort_by_key(|record| (record.object_identity_sha256.clone(), record.shard_ordinal));
    aggregate
        .blobs
        .sort_by_key(|record| (record.object_identity_sha256.clone(), record.blob_ordinal));
    aggregate
        .logical_extents
        .sort_by_key(|record| (record.object_identity_sha256.clone(), record.extent_ordinal));
    aggregate.physical_extents.sort_by_key(|record| {
        (
            record.object_identity_sha256.clone(),
            record.blob_ordinal,
            record.extent_ordinal,
        )
    });
    aggregate.checksum_chunks.sort_by_key(|record| {
        (
            record.object_ordinal,
            record.blob_ordinal,
            record.checksum_ordinal,
        )
    });

    aggregate.scan.o_latest_count = 4;
    aggregate.scan.o_decoded_count = 4;
    aggregate.scan.object_count = 2;
    aggregate.scan.blob_count = 2;
    aggregate.scan.onode_shard_count = 2;
    aggregate.scan.logical_extent_count = 4;
    aggregate.scan.physical_extent_count = 2;
    aggregate.scan.checksum_chunk_count = 2;
    aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(aggregate);
}

fn table_count(conn: &Connection, table: &str) -> u64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .expect("query table count")
}

fn expand_checksum_chunks(aggregate: &mut CephBluestoreSemanticAggregate, checksum_count: u32) {
    let chunk_size = aggregate.blobs[0]
        .checksum_chunk_size
        .expect("checksum chunk size");
    let total_length = chunk_size * u64::from(checksum_count);
    let template = aggregate.checksum_chunks[0].clone();
    aggregate.checksum_chunks = (0..checksum_count)
        .map(|ordinal| CephBluestoreChecksumChunkRecord {
            object_ordinal: template.object_ordinal,
            blob_ordinal: template.blob_ordinal,
            checksum_ordinal: ordinal,
            chunk_offset: u64::from(ordinal) * chunk_size,
            chunk_length: chunk_size,
            checksum_value: template.checksum_value,
            checksum_value_bytes: template.checksum_value_bytes,
        })
        .collect();
    aggregate.blobs[0].logical_length = total_length;
    aggregate.blobs[0].on_disk_length = total_length;
    aggregate.blobs[0].checksum_encoded_length = Some(u64::from(checksum_count) * 4);
    aggregate.blobs[0].checksum_value_count = u64::from(checksum_count);
    aggregate.blobs[0].logical_extent_count = 1;
    aggregate.objects[0].size = total_length;
    aggregate.objects[0].expected_object_size = total_length;
    aggregate.objects[0].expected_write_size = total_length;
    aggregate.objects[0].logical_extent_count = 1;
    aggregate.onode_shards[0].declared_extent_count = Some(1);
    aggregate.onode_shards[0].logical_extent_count = 1;
    aggregate.logical_extents.truncate(1);
    aggregate.logical_extents[0].length = total_length;
    aggregate.physical_extents[0].length = total_length;
    aggregate.shared_blobs[0].total_ref_bytes = total_length;
    aggregate.shared_blob_refs[0].length = total_length;
    aggregate.scan.logical_extent_count = 1;
    aggregate.scan.checksum_chunk_count = u64::from(checksum_count);
    aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(aggregate);
}

#[test]
fn ceph_bluestore_semantic_schema_is_normalized_and_raw_free() {
    let conn = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_018_cephfs_metadata_inventory"
    );
    let tables = [
        "ceph_bluestore_semantic_scans",
        "ceph_bluestore_super",
        "ceph_bluestore_collections",
        "ceph_bluestore_objects",
        "ceph_bluestore_onode_shards",
        "ceph_bluestore_blobs",
        "ceph_bluestore_logical_extents",
        "ceph_bluestore_physical_extents",
        "ceph_bluestore_checksum_chunks",
        "ceph_bluestore_shared_blobs",
        "ceph_bluestore_shared_blob_refs",
    ];
    let forbidden = [
        "key",
        "value",
        "raw_key",
        "raw_value",
        "encoded_payload",
        "checksum_bytes",
        "checksum_value",
    ];
    for table in tables {
        let columns = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("prepare columns")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");
        assert!(
            columns
                .iter()
                .all(|column| !forbidden.contains(&column.as_str())),
            "{table} contains a raw key/value or checksum payload column"
        );
    }
    let super_columns = conn
        .prepare("SELECT name FROM pragma_table_info('ceph_bluestore_super')")
        .expect("prepare super columns")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query super columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect super columns");
    for required in [
        "nid_max",
        "blobid_max",
        "min_alloc_size",
        "ondisk_format",
        "min_compat_ondisk_format",
        "per_pool_omap",
        "freelist_type",
        "observed_count",
        "deferred_count",
    ] {
        assert!(super_columns.contains(&required.to_string()));
    }
    let checksum_columns = conn
        .prepare("SELECT name FROM pragma_table_info('ceph_bluestore_checksum_chunks')")
        .expect("prepare checksum columns")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query checksum columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect checksum columns");
    assert_eq!(
        checksum_columns,
        vec![
            "inventory_id".to_string(),
            "object_identity_sha256".to_string(),
            "blob_ordinal".to_string(),
            "checksum_ordinal".to_string(),
            "chunk_offset".to_string(),
            "chunk_length".to_string(),
            "checksum_value_hex".to_string(),
        ]
    );
}

#[test]
fn ceph_bluestore_semantic_complete_roundtrip_preserves_typed_rows() {
    let conn = setup();
    let expected = aggregate(INVENTORY_A);
    validate_replacement(&expected).expect("validate aggregate");
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("replace aggregate");

    let actual = CephBluestoreSemanticRepo::new(&conn)
        .find_aggregate(INVENTORY_A)
        .expect("query aggregate")
        .expect("aggregate exists");
    assert_eq!(actual, expected);
    assert_eq!(
        actual.physical_extents[0].physical_offset_hex.as_deref(),
        Some("0000000000001000")
    );
    assert_eq!(actual.checksum_chunks[0].checksum_value, 0x1234_5678);
    assert_eq!(actual.checksum_chunks[0].checksum_value_bytes, 4);
    assert_eq!(actual.objects[0].object_namespace, b"ns\0");

    let mut compressed = aggregate(INVENTORY_B);
    compressed.blobs[0].compressed_length = Some(2048);
    compressed.blobs[0].flags_raw = 22;
    compressed.blobs[0].flag_compressed = true;
    compressed.physical_extents[0].physical_offset_hex = None;
    compressed.scan.semantic_sha256 = semantic_aggregate_sha256(&compressed);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&compressed)
        .expect("replace compressed aggregate");
    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_B)
            .unwrap(),
        Some(compressed)
    );
}

#[test]
fn targeted_object_read_plan_and_exact_candidate_are_stable() {
    let conn = setup();
    let expected = aggregate(INVENTORY_A);
    let object = expected.objects[0].clone();
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert semantic aggregate");

    let repo = CephBluestoreSemanticRepo::new(&conn);
    let plan = repo
        .find_object_read_plan(INVENTORY_A, &object.object_identity_sha256)
        .expect("query object read plan")
        .expect("object read plan exists");
    assert_eq!(plan.inventory_id, INVENTORY_A);
    assert_eq!(plan.object_identity_sha256, object.object_identity_sha256);
    assert_eq!(plan.object_ordinal, 0);
    assert_eq!(plan.object, object);
    assert_eq!(plan.blobs, expected.blobs);
    assert_eq!(plan.logical_extents, expected.logical_extents);
    assert_eq!(plan.physical_extents, expected.physical_extents);
    assert_eq!(plan.checksum_chunks, expected.checksum_chunks);

    let candidate = repo
        .find_object_candidate(
            INVENTORY_A,
            &object.object_name,
            object.decoded_pool,
            &object.object_namespace,
            &object.snap_hex,
        )
        .expect("query exact object candidate")
        .expect("candidate exists");
    assert_eq!(candidate.inventory_id, INVENTORY_A);
    assert_eq!(
        candidate.object_identity_sha256,
        object.object_identity_sha256
    );
    assert_eq!(candidate.object_name, object.object_name);
    assert_eq!(candidate.decoded_pool, object.decoded_pool);
    assert_eq!(candidate.object_namespace, object.object_namespace);
    assert_eq!(candidate.snap_hex, object.snap_hex);

    let session =
        CephBluestoreReadPlanSession::new(conn, INVENTORY_A).expect("prepare read-plan session");
    assert!(
        !session.connection().is_autocommit(),
        "read-plan session should keep one stable read snapshot"
    );
    let session_candidate = session
        .find_object_candidate(
            &object.object_name,
            object.decoded_pool,
            &object.object_namespace,
            &object.snap_hex,
        )
        .expect("query candidate through session")
        .expect("session candidate exists");
    let session_plan = session
        .find_object_read_plan(&object.object_identity_sha256)
        .expect("query plan through session")
        .expect("session plan exists");

    assert_eq!(session_candidate, candidate);
    assert_eq!(session_plan, plan);
}

#[test]
fn targeted_object_read_does_not_load_unrelated_rocksdb_children() {
    let conn = setup();
    let expected = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert semantic aggregate");

    conn.execute_batch("PRAGMA ignore_check_constraints = ON;")
        .expect("disable checks for corruption fixture");
    conn.execute(
        "INSERT INTO ceph_rocksdb_live_files (
            inventory_id, column_family_id, level, file_number, path_id,
            format, file_size, smallest_sequence, largest_sequence,
            smallest_internal_key_length, largest_internal_key_length
         ) VALUES (?1, 0, 0, 149, 0, 'corrupt', 1, NULL, NULL, 8, 8)",
        [INVENTORY_A],
    )
    .expect("insert corrupt unrelated RocksDB child");
    conn.execute(
        "UPDATE ceph_rocksdb_latest_state
         SET point_mutation_count = -1
         WHERE inventory_id = ?1",
        [INVENTORY_A],
    )
    .expect("corrupt unrelated latest-state payload");
    conn.execute_batch("PRAGMA ignore_check_constraints = OFF;")
        .expect("restore checks");

    let object = &expected.objects[0];
    let plan = CephBluestoreSemanticRepo::new(&conn)
        .find_object_read_plan(INVENTORY_A, &object.object_identity_sha256)
        .expect("query object read plan")
        .expect("object read plan exists");
    assert_eq!(plan.object, *object);
}

#[test]
fn targeted_object_read_does_not_count_unrelated_semantic_children() {
    let conn = setup();
    let mut expected = aggregate(INVENTORY_A);
    append_second_object(&mut expected, "0000000000002000", false);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert multiple objects");

    let target = expected
        .objects
        .iter()
        .find(|object| object.object_name == b"object")
        .expect("target object");
    let unrelated = expected
        .objects
        .iter()
        .find(|object| object.object_name == b"second-object")
        .expect("unrelated object");
    conn.execute(
        "DELETE FROM ceph_bluestore_physical_extents
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2",
        params![INVENTORY_A, unrelated.object_identity_sha256],
    )
    .expect("delete unrelated physical child");

    let plan = CephBluestoreSemanticRepo::new(&conn)
        .find_object_read_plan(INVENTORY_A, &target.object_identity_sha256)
        .expect("query object read plan")
        .expect("object read plan exists");
    assert_eq!(plan.object, *target);
}

#[test]
fn targeted_object_read_plan_uses_local_checksum_ordinals() {
    let conn = setup();
    let mut expected = aggregate(INVENTORY_A);
    append_second_object(&mut expected, "0000000000002000", false);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert multiple objects");

    let target = expected
        .objects
        .iter()
        .find(|object| object.object_name == b"second-object")
        .expect("second object");
    let plan = CephBluestoreSemanticRepo::new(&conn)
        .find_object_read_plan(INVENTORY_A, &target.object_identity_sha256)
        .expect("query targeted object")
        .expect("targeted object exists");

    assert_eq!(plan.object, *target);
    assert_eq!(plan.object_ordinal, 0);
    assert!(plan
        .checksum_chunks
        .iter()
        .all(|record| record.object_ordinal == 0));
}

#[test]
fn targeted_object_reads_return_none_for_valid_missing_keys() {
    let conn = setup();
    let expected = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert semantic aggregate");
    let repo = CephBluestoreSemanticRepo::new(&conn);

    assert_eq!(
        repo.find_object_read_plan(INVENTORY_A, &"f".repeat(64))
            .expect("query missing object"),
        None
    );
    assert_eq!(
        repo.find_object_candidate(INVENTORY_A, b"missing", 7, b"ns\0", RBD_HEAD_SNAP_HEX,)
            .expect("query missing candidate"),
        None
    );
}

#[test]
fn exact_object_candidate_rejects_ambiguous_matches() {
    let conn = setup();
    let mut expected = aggregate(INVENTORY_A);
    append_second_object(&mut expected, "0000000000002000", false);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert multiple objects");
    let second_id = expected
        .objects
        .iter()
        .find(|object| object.object_name == b"second-object")
        .expect("second object")
        .object_identity_sha256
        .clone();
    conn.execute(
        "UPDATE ceph_bluestore_objects
         SET object_name = ?1
         WHERE inventory_id = ?2 AND object_identity_sha256 = ?3",
        params![b"object".as_slice(), INVENTORY_A, second_id],
    )
    .expect("make exact lookup ambiguous");

    let object = expected
        .objects
        .iter()
        .find(|object| object.object_name == b"object")
        .expect("original object");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .find_object_candidate(
            INVENTORY_A,
            &object.object_name,
            object.decoded_pool,
            &object.object_namespace,
            &object.snap_hex,
        )
        .is_err());
}

#[test]
fn targeted_object_reads_fail_closed_for_corrupt_binding_and_ranges() {
    let conn = setup();
    let expected = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert semantic aggregate");
    conn.execute(
        "UPDATE ceph_bluestore_physical_extents
         SET physical_offset_hex = ?1
         WHERE inventory_id = ?2",
        params!["0000000000100000", INVENTORY_A],
    )
    .expect("corrupt physical range");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .find_object_read_plan(INVENTORY_A, &expected.objects[0].object_identity_sha256)
        .is_err());

    let conn = setup();
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("insert second semantic aggregate");
    conn.execute(
        "UPDATE ceph_rocksdb_latest_state
         SET sharding_sha256 = ?1
         WHERE inventory_id = ?2",
        params!["e".repeat(64), INVENTORY_A],
    )
    .expect("corrupt persisted recovery binding");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .find_object_read_plan(INVENTORY_A, &expected.objects[0].object_identity_sha256)
        .is_err());
}

#[test]
fn batched_checksum_roundtrip_preserves_rows_and_object_ordinals() {
    let conn = setup();
    let mut expected = aggregate(INVENTORY_A);
    expand_checksum_chunks(&mut expected, 129);
    validate_replacement(&expected).expect("validate batched aggregate");

    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("write more than one checksum batch");
    let actual = CephBluestoreSemanticRepo::new(&conn)
        .find_aggregate(INVENTORY_A)
        .expect("query batched aggregate")
        .expect("batched aggregate exists");

    assert_eq!(actual, expected);
    assert_eq!(actual.checksum_chunks[0].object_ordinal, 0);
    assert_eq!(actual.checksum_chunks[128].object_ordinal, 0);
}

#[test]
fn ceph_bluestore_semantic_replacement_is_source_local() {
    let conn = setup();
    let first = aggregate(INVENTORY_A);
    let second = aggregate(INVENTORY_B);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&first)
        .expect("insert first");
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&second)
        .expect("insert second");

    let mut replacement = first.clone();
    replacement.super_record.nid_max = Some(101);
    replacement.scan.semantic_sha256 = semantic_aggregate_sha256(&replacement);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&replacement)
        .expect("replace first");

    let repo = CephBluestoreSemanticRepo::new(&conn);
    assert_eq!(repo.find_aggregate(INVENTORY_A).unwrap(), Some(replacement));
    assert_eq!(repo.find_aggregate(INVENTORY_B).unwrap(), Some(second));
}

#[test]
fn ceph_bluestore_semantic_invalid_inputs_preserve_old_snapshot() {
    let conn = setup();
    let baseline = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&baseline)
        .expect("insert baseline");

    let mut invalid_cases = Vec::new();
    let mut invalid = baseline.clone();
    invalid.scan.object_count = 2;
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.scan.semantic_sha256 = "A".repeat(64);
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.scan.semantic_sha256 = "f".repeat(64);
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.logical_extents[1].logical_offset = 1024;
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.logical_extents[0].blob_ordinal = 99;
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.blobs[0].inventory_id = INVENTORY_B.to_string();
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.scan.profile_complete = false;
    invalid_cases.push(invalid);
    let mut invalid = baseline.clone();
    invalid.super_record.min_alloc_size = Some(i64::MAX as u64 + 1);
    invalid_cases.push(invalid);

    for invalid in invalid_cases {
        assert!(CephBluestoreSemanticRepo::new(&conn)
            .replace_for_inventory(&invalid)
            .is_err());
        assert_eq!(
            CephBluestoreSemanticRepo::new(&conn)
                .find_aggregate(INVENTORY_A)
                .unwrap(),
            Some(baseline.clone())
        );
    }

    let missing_parent = aggregate("missing-inventory");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&missing_parent)
        .is_err());
    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_A)
            .unwrap(),
        Some(baseline)
    );
}

#[test]
fn ceph_bluestore_semantic_inventory_delete_cascades_all_rows() {
    let conn = setup();
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&aggregate(INVENTORY_A))
        .expect("insert aggregate");

    conn.execute(
        "DELETE FROM ceph_osd_inventory WHERE id = ?1",
        [INVENTORY_A],
    )
    .expect("delete inventory");
    for table in [
        "ceph_bluestore_semantic_scans",
        "ceph_bluestore_super",
        "ceph_bluestore_collections",
        "ceph_bluestore_objects",
        "ceph_bluestore_onode_shards",
        "ceph_bluestore_blobs",
        "ceph_bluestore_logical_extents",
        "ceph_bluestore_physical_extents",
        "ceph_bluestore_checksum_chunks",
        "ceph_bluestore_shared_blobs",
        "ceph_bluestore_shared_blob_refs",
    ] {
        assert_eq!(table_count(&conn, table), 0, "{table} did not cascade");
    }
}

#[test]
fn ceph_bluestore_semantic_public_replacement_rolls_back_group_failure() {
    let conn = setup();
    let baseline = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&baseline)
        .expect("insert baseline");
    conn.execute_batch(
        "CREATE TRIGGER fail_semantic_physical
         BEFORE INSERT ON ceph_bluestore_physical_extents
         BEGIN
             SELECT RAISE(ABORT, 'injected semantic write failure');
         END;",
    )
    .expect("install failure trigger");

    let mut replacement = baseline.clone();
    replacement.super_record.nid_max = Some(101);
    expand_checksum_chunks(&mut replacement, 129);
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&replacement)
        .is_err());

    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_A)
            .unwrap(),
        Some(baseline)
    );
}

#[test]
fn standalone_semantic_replacement_enforces_recovery_and_device_binding() {
    let conn = setup();
    let baseline = aggregate(INVENTORY_A);
    CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&baseline)
        .expect("insert baseline");

    let mut wrong_latest_state = baseline.clone();
    wrong_latest_state.scan.latest_state_sha256 = "f".repeat(64);
    wrong_latest_state.scan.semantic_sha256 = semantic_aggregate_sha256(&wrong_latest_state);
    validate_replacement(&wrong_latest_state).expect("aggregate is internally valid");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&wrong_latest_state)
        .is_err());

    let mut wrong_sharding = baseline.clone();
    wrong_sharding.scan.sharding_sha256 = "e".repeat(64);
    wrong_sharding.scan.semantic_sha256 = semantic_aggregate_sha256(&wrong_sharding);
    validate_replacement(&wrong_sharding).expect("aggregate is internally valid");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&wrong_sharding)
        .is_err());

    let mut out_of_bounds = baseline.clone();
    out_of_bounds.physical_extents[0].physical_offset_hex = Some("0000000000100000".to_string());
    out_of_bounds.shared_blob_refs[0].ref_offset_hex = "0000000000100000".to_string();
    out_of_bounds.scan.semantic_sha256 = semantic_aggregate_sha256(&out_of_bounds);
    validate_replacement(&out_of_bounds).expect("aggregate is internally valid");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&out_of_bounds)
        .is_err());

    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_A)
            .expect("reload baseline"),
        Some(baseline)
    );

    conn.execute(
        "DELETE FROM ceph_rocksdb_latest_state WHERE inventory_id = ?1",
        [INVENTORY_B],
    )
    .expect("remove latest state");
    assert!(CephBluestoreSemanticRepo::new(&conn)
        .replace_for_inventory(&aggregate(INVENTORY_B))
        .is_err());
}

#[test]
fn checksum_chunks_require_canonical_fixed_width_complete_coverage() {
    let baseline = aggregate(INVENTORY_A);
    let mut invalid_cases = Vec::new();

    let mut invalid = baseline.clone();
    invalid.checksum_chunks[0].checksum_value = 0x1ff;
    invalid.checksum_chunks[0].checksum_value_bytes = 1;
    invalid_cases.push(invalid);

    let mut invalid = baseline.clone();
    invalid.checksum_chunks[0].checksum_value_bytes = 2;
    invalid_cases.push(invalid);

    let mut invalid = baseline.clone();
    invalid.checksum_chunks[0].chunk_offset = 1;
    invalid.checksum_chunks[0].chunk_length = 4095;
    invalid_cases.push(invalid);

    let mut invalid = baseline.clone();
    invalid.checksum_chunks[0].object_ordinal = 1;
    invalid_cases.push(invalid);

    let mut invalid = baseline.clone();
    expand_checksum_chunks(&mut invalid, 2);
    invalid.checksum_chunks.swap(0, 1);
    invalid_cases.push(invalid);

    let mut invalid = baseline;
    invalid.checksum_chunks.clear();
    invalid.scan.checksum_chunk_count = 0;
    invalid_cases.push(invalid);

    for mut invalid in invalid_cases {
        invalid.scan.semantic_sha256 = semantic_aggregate_sha256(&invalid);
        assert!(validate_replacement(&invalid).is_err());
    }
}

#[test]
fn physical_overlap_accepts_partial_slices_with_common_shared_blob() {
    let mut exact_shared = aggregate(INVENTORY_A);
    append_second_object(&mut exact_shared, "0000000000001000", true);
    exact_shared.shared_blobs[0].total_refs = 2;
    exact_shared.shared_blob_refs[0].refs = 2;
    exact_shared.scan.semantic_sha256 = semantic_aggregate_sha256(&exact_shared);
    validate_replacement(&exact_shared).expect("exact shared range is valid");

    let mut partial = aggregate(INVENTORY_A);
    append_second_object(&mut partial, "0000000000001800", true);
    partial.shared_blob_refs = vec![
        shared_ref(INVENTORY_A, 0, 0x1000, 0x800, 1),
        shared_ref(INVENTORY_A, 1, 0x1800, 0x800, 2),
        shared_ref(INVENTORY_A, 2, 0x2000, 0x800, 1),
    ];
    partial.shared_blobs[0].ref_extent_count = 3;
    partial.shared_blobs[0].total_ref_bytes = 0x1800;
    partial.shared_blobs[0].total_refs = 4;
    partial.scan.shared_ref_extent_count = 3;
    partial.scan.semantic_sha256 = semantic_aggregate_sha256(&partial);
    validate_replacement(&partial).expect("shared ref map covers both partial blob slices");
}

#[test]
fn physical_overlap_rejects_unrelated_shared_identities_and_blob_self_overlap() {
    let mut unrelated = aggregate(INVENTORY_A);
    append_second_object(&mut unrelated, "0000000000001000", false);
    assert!(validate_replacement(&unrelated).is_err());

    let mut different_shared = aggregate(INVENTORY_A);
    append_second_object(&mut different_shared, "0000000000001000", true);
    let second_object = different_shared
        .objects
        .iter()
        .find(|object| object.object_name == b"second-object")
        .expect("second object")
        .object_identity_sha256
        .clone();
    let second_blob = different_shared
        .blobs
        .iter_mut()
        .find(|blob| blob.object_identity_sha256 == second_object)
        .expect("second object blob");
    second_blob.shared_blob_id_hex = Some("9977665544332211".to_string());
    different_shared
        .shared_blobs
        .push(CephBluestoreSharedBlobRecord {
            inventory_id: INVENTORY_A.to_string(),
            shared_blob_id_hex: "9977665544332211".to_string(),
            denc_version: Some(1),
            decode_status: "parsed".to_string(),
            deferred_reason: None,
            ref_extent_count: 1,
            total_ref_bytes: 4096,
            total_refs: 1,
        });
    different_shared
        .shared_blob_refs
        .push(shared_ref(INVENTORY_A, 0, 0x1000, 4096, 1));
    different_shared.shared_blob_refs[1].shared_blob_id_hex = "9977665544332211".to_string();
    different_shared.scan.x_latest_count = 2;
    different_shared.scan.x_decoded_count = 2;
    different_shared.scan.shared_blob_count = 2;
    different_shared.scan.shared_ref_extent_count = 2;
    different_shared.scan.semantic_sha256 = semantic_aggregate_sha256(&different_shared);
    assert!(validate_replacement(&different_shared).is_err());

    let mut duplicate_within_blob = aggregate(INVENTORY_A);
    duplicate_within_blob.blobs[0].logical_length = 8192;
    duplicate_within_blob.blobs[0].on_disk_length = 8192;
    duplicate_within_blob.blobs[0].checksum_encoded_length = Some(8);
    duplicate_within_blob.blobs[0].checksum_value_count = 2;
    duplicate_within_blob.blobs[0].physical_extent_count = 2;
    duplicate_within_blob.objects[0].physical_extent_count = 2;
    let mut physical = duplicate_within_blob.physical_extents[0].clone();
    physical.extent_ordinal = 1;
    physical.blob_offset = 4096;
    duplicate_within_blob.physical_extents.push(physical);
    let mut checksum = duplicate_within_blob.checksum_chunks[0].clone();
    checksum.checksum_ordinal = 1;
    checksum.chunk_offset = 4096;
    duplicate_within_blob.checksum_chunks.push(checksum);
    duplicate_within_blob.scan.physical_extent_count = 2;
    duplicate_within_blob.scan.checksum_chunk_count = 2;
    duplicate_within_blob.scan.semantic_sha256 = semantic_aggregate_sha256(&duplicate_within_blob);
    assert!(validate_replacement(&duplicate_within_blob).is_err());
}

#[test]
fn shared_physical_extent_requires_complete_ref_map_coverage() {
    let mut outside = aggregate(INVENTORY_A);
    outside.shared_blob_refs[0].length = 2048;
    outside.shared_blobs[0].total_ref_bytes = 2048;
    outside.scan.semantic_sha256 = semantic_aggregate_sha256(&outside);
    assert!(validate_replacement(&outside).is_err());
}

fn shared_ref(
    inventory_id: &str,
    ordinal: u32,
    offset: u64,
    length: u64,
    refs: u64,
) -> CephBluestoreSharedBlobRefRecord {
    CephBluestoreSharedBlobRefRecord {
        inventory_id: inventory_id.to_string(),
        shared_blob_id_hex: "8877665544332211".to_string(),
        ref_ordinal: ordinal,
        ref_offset_hex: format!("{offset:016x}"),
        length,
        refs,
    }
}
