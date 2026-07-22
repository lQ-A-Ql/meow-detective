use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluestore_semantic_repo::{
            latest_state_set_sha256, object_identity_sha256, CephBluestoreObjectPageCursor,
            CephBluestoreObjectRecord, CephBluestoreSemanticRepo,
        },
        ceph_fs_metadata_inventory_repo::{
            cephfs_metadata_inventory_sha256, CephFsMetadataInventory,
            CephFsMetadataInventoryManifest, CephFsMetadataInventoryRepo,
            CephFsMetadataInventoryRepoError, CephFsMetadataObjectProjection,
            CephFsMetadataWriteOutcome, CEPHFS_METADATA_CLASSIFIER_PROFILE,
            CEPHFS_METADATA_SCHEMA_VERSION,
        },
        ceph_rocksdb_latest_state_repo::{
            CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo,
        },
    },
    runner,
};
use rusqlite::{params, Connection};

const INVENTORY: &str = "inventory-a";
const SOURCE: &str = "source-a";
const FILESYSTEM: &str = "ceph-fs:cluster-a:1:17:7";
const HEAD_SNAP: &str = "fffffffffffffffe";

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    seed_control_plane(&conn);
    seed_semantic_scan(&conn);
    conn
}

fn seed_control_plane(conn: &Connection) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-19T00:00:00Z')",
        [SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (?1, ?2, ?1, 'block', 1048576, 1, 0, 'BlueStore OSD', 1,
                   1, 'singleReplica', 1, '{}')",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
                   4096, 1, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [INVENTORY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
                   'leveldb.BytewiseComparator', 100, 150, 142, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped, log_number
         ) VALUES (?1, 0, 'default', 'leveldb.BytewiseComparator', 0, 142)",
        [INVENTORY],
    )
    .unwrap();
    CephRocksdbLatestStateRepo::new(conn)
        .replace_for_inventory(INVENTORY, &latest_state())
        .unwrap();
}

fn seed_semantic_scan(conn: &Connection) {
    conn.execute(
        "INSERT INTO ceph_bluestore_semantic_scans (
            inventory_id, schema_version, decode_profile, sharding_sha256,
            latest_state_sha256, semantic_sha256,
            s_latest_count, s_decoded_count, s_deferred_count,
            c_latest_count, c_decoded_count, c_deferred_count,
            o_latest_count, o_decoded_count, o_deferred_count,
            x_latest_count, x_decoded_count, x_deferred_count,
            collection_count, object_count, blob_count, onode_shard_count,
            logical_extent_count, physical_extent_count, checksum_chunk_count,
            shared_blob_count, shared_ref_extent_count, profile_complete
         ) VALUES (?1, 1, 'scox-v1', ?2, ?3, ?4,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 1)",
        params![
            INVENTORY,
            "a".repeat(64),
            latest_state_set_sha256(&latest_state()),
            "c".repeat(64),
        ],
    )
    .unwrap();
}

fn latest_state() -> Vec<CephRocksdbLatestStateRecord> {
    vec![CephRocksdbLatestStateRecord {
        inventory_id: INVENTORY.to_string(),
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

fn insert_object(conn: &Connection, pool: i64, name: &[u8], namespace: &[u8]) -> String {
    let mut object = CephBluestoreObjectRecord {
        inventory_id: INVENTORY.to_string(),
        object_identity_sha256: String::new(),
        decoded_shard: -1,
        decoded_pool: pool,
        decoded_hash: 1,
        decoded_bitwise_hash: 0x8000_0000,
        object_namespace: namespace.to_vec(),
        object_key: None,
        object_name: name.to_vec(),
        snap_hex: HEAD_SNAP.to_string(),
        generation_hex: "0000000000000000".to_string(),
        onode_denc_version: 1,
        nid: 1,
        size: 32,
        flags_raw: 0,
        flag_omap: false,
        flag_pgmeta_omap: false,
        flag_per_pool_omap: false,
        flag_per_pg_omap: false,
        flags_unknown_bits: 0,
        attribute_count: 0,
        attribute_value_bytes: 0,
        attributes_sha256: "e".repeat(64),
        expected_object_size: 32,
        expected_write_size: 32,
        allocation_hint_flags: 0,
        zone_ref_count: 0,
        extent_storage: "inline".to_string(),
        spanning_blob_version: 0,
        declared_spanning_blob_count: 0,
        decode_status: "parsed".to_string(),
        deferred_reason: None,
        onode_shard_count: 0,
        blob_count: 0,
        logical_extent_count: 0,
        physical_extent_count: 0,
    };
    object.object_identity_sha256 = object_identity_sha256(&object);
    conn.execute(
        "INSERT INTO ceph_bluestore_objects (
            inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
            decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
            object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
            flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
            flag_per_pg_omap, flags_unknown_bits, attribute_count,
            attribute_value_bytes, attributes_sha256, expected_object_size,
            expected_write_size, allocation_hint_flags, zone_ref_count,
            extent_storage, spanning_blob_version, declared_spanning_blob_count,
            decode_status, deferred_reason, onode_shard_count, blob_count,
            logical_extent_count, physical_extent_count
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
            ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36)",
        params![
            object.inventory_id,
            object.object_identity_sha256,
            object.decoded_shard,
            object.decoded_pool,
            object.decoded_hash,
            object.decoded_bitwise_hash,
            object.object_namespace,
            object.object_key,
            object.object_name,
            object.snap_hex,
            object.generation_hex,
            object.onode_denc_version,
            object.nid,
            object.size,
            object.flags_raw,
            object.flag_omap,
            object.flag_pgmeta_omap,
            object.flag_per_pool_omap,
            object.flag_per_pg_omap,
            object.flags_unknown_bits,
            object.attribute_count,
            object.attribute_value_bytes,
            object.attributes_sha256,
            object.expected_object_size,
            object.expected_write_size,
            object.allocation_hint_flags,
            object.zone_ref_count,
            object.extent_storage,
            object.spanning_blob_version,
            object.declared_spanning_blob_count,
            object.decode_status,
            object.deferred_reason,
            object.onode_shard_count,
            object.blob_count,
            object.logical_extent_count,
            object.physical_extent_count,
        ],
    )
    .unwrap();
    object.object_identity_sha256
}

fn inventory(object_id: &str, record_digest: char, pool: i64) -> CephFsMetadataInventory {
    let mut inventory = CephFsMetadataInventory {
        manifest: CephFsMetadataInventoryManifest {
            filesystem_identity: FILESYSTEM.to_string(),
            inventory_id: INVENTORY.to_string(),
            data_source_id: SOURCE.to_string(),
            filesystem_id: 1,
            fsmap_epoch: 17,
            metadata_pool_id: pool,
            schema_version: CEPHFS_METADATA_SCHEMA_VERSION,
            classifier_profile: CEPHFS_METADATA_CLASSIFIER_PROFILE.to_string(),
            source_semantic_sha256: "c".repeat(64),
            inventory_sha256: String::new(),
            object_count: 1,
            unknown_object_count: 0,
            complete: true,
        },
        objects: vec![CephFsMetadataObjectProjection {
            object_identity_sha256: object_id.to_string(),
            locator: "1:7:h:h312e3030303030303030:17".to_string(),
            candidate_mask: 7,
            classification_state: "candidate".to_string(),
            classifier_rule: "dirfrag_candidate".to_string(),
            record_sha256: record_digest.to_string().repeat(64),
        }],
    };
    inventory.manifest.inventory_sha256 =
        cephfs_metadata_inventory_sha256(&inventory.manifest, &inventory.objects);
    inventory
}

#[test]
fn pool_pages_are_keyset_ordered_binary_safe_and_isolated() {
    let conn = setup();
    let first = insert_object(&conn, 7, b"1.00000000", &[0xff]);
    let second = insert_object(&conn, 7, b"2.00000000", b"");
    insert_object(&conn, 8, b"other-pool", b"");
    let repo = CephBluestoreSemanticRepo::new(&conn);

    let first_page = repo
        .list_objects_by_pool_after(INVENTORY, 7, None, 1)
        .unwrap();
    assert_eq!(first_page.objects.len(), 1);
    let cursor = first_page.next_cursor.expect("next cursor");
    let second_page = repo
        .list_objects_by_pool_after(INVENTORY, 7, Some(&cursor), 1)
        .unwrap();
    assert_eq!(second_page.objects.len(), 1);
    assert_ne!(
        first_page.objects[0].object_identity_sha256,
        second_page.objects[0].object_identity_sha256
    );
    assert!(first_page
        .objects
        .iter()
        .chain(&second_page.objects)
        .any(|object| object.object_namespace == vec![0xff]));
    assert_eq!(
        [first, second]
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        [
            first_page.objects[0].object_identity_sha256.clone(),
            second_page.objects[0].object_identity_sha256.clone(),
        ]
        .into_iter()
        .collect()
    );
    assert!(CephBluestoreObjectPageCursor::new("X".repeat(64)).is_err());
    assert!(repo
        .list_objects_by_pool_after(INVENTORY, 7, None, 0)
        .is_err());
}

#[test]
fn inventory_replace_is_idempotent_and_conflicts_fail_closed() {
    let conn = setup();
    let object_id = insert_object(&conn, 7, b"1.00000000", b"");
    let repo = CephFsMetadataInventoryRepo::new(&conn);
    let first = inventory(&object_id, 'a', 7);
    assert_eq!(
        repo.replace(&first).unwrap(),
        CephFsMetadataWriteOutcome::Replaced
    );
    assert_eq!(
        repo.replace(&first).unwrap(),
        CephFsMetadataWriteOutcome::Unchanged
    );
    let conflicting = inventory(&object_id, 'b', 7);
    assert!(matches!(
        repo.replace(&conflicting),
        Err(CephFsMetadataInventoryRepoError::DeterminismConflict)
    ));
    assert_eq!(repo.find(FILESYSTEM, INVENTORY).unwrap(), Some(first));
}

#[test]
fn inventory_replace_rejects_a_digest_that_does_not_cover_its_projections() {
    let conn = setup();
    let object_id = insert_object(&conn, 7, b"1.00000000", b"");
    let repo = CephFsMetadataInventoryRepo::new(&conn);
    let mut invalid = inventory(&object_id, 'a', 7);
    invalid.manifest.inventory_sha256 = "f".repeat(64);

    assert!(matches!(
        repo.replace(&invalid),
        Err(CephFsMetadataInventoryRepoError::Invalid(
            "manifest inventory digest does not match projections"
        ))
    ));
    assert!(repo.find(FILESYSTEM, INVENTORY).unwrap().is_none());
}

#[test]
fn cross_pool_projection_rolls_back_and_source_delete_cascades() {
    let conn = setup();
    let object_id = insert_object(&conn, 8, b"1.00000000", b"");
    let repo = CephFsMetadataInventoryRepo::new(&conn);
    assert!(matches!(
        repo.replace(&inventory(&object_id, 'a', 7)),
        Err(CephFsMetadataInventoryRepoError::CrossPoolReference)
    ));
    assert!(repo.find(FILESYSTEM, INVENTORY).unwrap().is_none());

    let same_pool_id = insert_object(&conn, 8, b"2.00000000", b"");
    let mut stored = inventory(&same_pool_id, 'b', 8);
    stored.objects[0].locator = "1:8:h:h322e3030303030303030:17".to_string();
    stored.manifest.inventory_sha256 =
        cephfs_metadata_inventory_sha256(&stored.manifest, &stored.objects);
    repo.replace(&stored).unwrap();
    conn.execute("DELETE FROM data_sources WHERE id = ?1", [SOURCE])
        .unwrap();
    assert!(repo.find(FILESYSTEM, INVENTORY).unwrap().is_none());
}

#[test]
fn source_migrations_are_current_and_reapplication_is_idempotent() {
    let conn = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_024_ntfs_deleted_recovery"
    );
    assert_eq!(runner::run_source_all(&conn).unwrap(), 0);
    let index_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_ceph_bluestore_objects_pool_identity'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 1);
}
