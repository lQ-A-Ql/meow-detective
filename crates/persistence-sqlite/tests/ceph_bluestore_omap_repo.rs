use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluestore_omap_repo::{
            canonical_scope_identity, omap_aggregate_sha256, CephBluestoreOmapAggregate,
            CephBluestoreOmapRepo, CephBluestoreOmapScanRecord, CephBluestoreOmapScopeRecord,
            CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
            BLUESTORE_OMAP_DECODE_PROFILE, BLUESTORE_OMAP_SCHEMA_VERSION,
        },
        ceph_bluestore_semantic_repo::{CephBluestoreSemanticAggregate, CephBluestoreSemanticRepo},
        ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRepo,
        ceph_rocksdb_repo::{
            CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
        },
    },
    runner,
};
use rusqlite::Connection;

mod support;

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    conn
}

fn seed_parent(
    conn: &Connection,
    inventory_id: &str,
    data_source_id: &str,
) -> (CephRocksdbAggregate, CephBluestoreSemanticAggregate) {
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
    .expect("insert OSD inventory");
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

    let rocksdb = rocksdb(inventory_id, data_source_id);
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, identity_uuid,
            manifest_file_number, manifest_file_size, logical_edit_count,
            comparator_name, last_sequence, next_file_number, log_number,
            prev_log_number, max_column_family_id, min_log_number_to_keep
         ) VALUES (
            ?1, ?2, ?3, NULL, ?4, 4096, 1,
            'leveldb.BytewiseComparator', 10, 12, 0, 0, 0, NULL
         )",
        rusqlite::params![
            inventory_id,
            data_source_id,
            rocksdb.manifest.active_manifest_path,
            rocksdb.manifest.manifest_file_number,
        ],
    )
    .expect("insert RocksDB manifest");
    conn.execute(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped, log_number
         ) VALUES (?1, 0, 'default', 'leveldb.BytewiseComparator', 0, NULL)",
        [inventory_id],
    )
    .expect("insert default column family");
    let latest_state = support::empty_latest_state(&rocksdb);
    CephRocksdbLatestStateRepo::new(conn)
        .replace_for_inventory(inventory_id, &latest_state)
        .expect("persist latest state");
    let semantic = support::empty_semantic(&rocksdb, &latest_state);
    CephBluestoreSemanticRepo::new(conn)
        .replace_for_inventory(&semantic)
        .expect("persist semantic parent");
    (rocksdb, semantic)
}

fn rocksdb(inventory_id: &str, data_source_id: &str) -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            active_manifest_path: "db/MANIFEST-000011".to_string(),
            identity_uuid: None,
            manifest_file_number: 11,
            manifest_file_size: 4096,
            logical_edit_count: 1,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 10,
            next_file_number: 12,
            log_number: 0,
            prev_log_number: 0,
            max_column_family_id: 0,
            min_log_number_to_keep: None,
        },
        column_families: vec![CephRocksdbColumnFamilyRecord {
            inventory_id: inventory_id.to_string(),
            column_family_id: 0,
            name: "default".to_string(),
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            dropped: false,
            log_number: None,
        }],
        live_ssts: Vec::new(),
    }
}

fn omap(
    rocksdb: &CephRocksdbAggregate,
    semantic: &CephBluestoreSemanticAggregate,
    image_name: &str,
    image_id: &str,
) -> CephBluestoreOmapAggregate {
    let inventory_id = rocksdb.manifest.inventory_id.clone();
    let directory_scope = scope(&inventory_id, 11, Some(("rbdDirectory", None)), 2, 2);
    let header_scope = scope(&inventory_id, 12, Some(("rbdHeader", Some(image_id))), 7, 7);
    let mut aggregate = CephBluestoreOmapAggregate {
        scan: CephBluestoreOmapScanRecord {
            inventory_id: inventory_id.clone(),
            data_source_id: rocksdb.manifest.data_source_id.clone(),
            schema_version: BLUESTORE_OMAP_SCHEMA_VERSION,
            decode_profile: BLUESTORE_OMAP_DECODE_PROFILE.to_string(),
            sharding_sha256: semantic.scan.sharding_sha256.clone(),
            latest_state_sha256: semantic.scan.latest_state_sha256.clone(),
            semantic_sha256: semantic.scan.semantic_sha256.clone(),
            omap_sha256: String::new(),
            scope_count: 2,
            directory_mapping_count: 1,
            rbd_header_count: 1,
            profile_complete: true,
        },
        scopes: vec![directory_scope.clone(), header_scope.clone()],
        directory_mappings: vec![CephBluestoreRbdDirectoryRecord {
            inventory_id: inventory_id.clone(),
            scope_identity: directory_scope.scope_identity,
            owner_nid_hex: hex_u64(11),
            image_name: image_name.to_string(),
            image_id: image_id.to_string(),
            bidirectional: true,
        }],
        rbd_headers: vec![CephBluestoreRbdHeaderRecord {
            inventory_id,
            scope_identity: header_scope.scope_identity,
            owner_nid_hex: hex_u64(12),
            image_id: image_id.to_string(),
            size_hex: Some(hex_u64(64 * 1024 * 1024)),
            object_order: Some(22),
            features_hex: Some(hex_u64(0x21)),
            operation_features_hex: None,
            parent_key_present: false,
            object_prefix: Some(format!("rbd_data.{image_id}")),
            stripe_unit_hex: Some(hex_u64(1 << 22)),
            stripe_count_hex: Some(hex_u64(1)),
            data_pool_id: Some(8),
        }],
    };
    aggregate.scan.omap_sha256 = omap_aggregate_sha256(&aggregate);
    aggregate
}

fn scope(
    inventory_id: &str,
    nid: u64,
    owner: Option<(&str, Option<&str>)>,
    entry_count: u64,
    recognized_entry_count: u64,
) -> CephBluestoreOmapScopeRecord {
    let nid_hex = hex_u64(nid);
    let (owner_kind, owner_image_id) = owner
        .map(|(kind, image_id)| (Some(kind.to_string()), image_id.map(str::to_string)))
        .unwrap_or((None, None));
    CephBluestoreOmapScopeRecord {
        inventory_id: inventory_id.to_string(),
        scope_identity: canonical_scope_identity("bulk", "none", None, None, None, &nid_hex)
            .expect("canonical scope"),
        key_family: "bulk".to_string(),
        pool_kind: "none".to_string(),
        pool_value_i64: None,
        pool_value_hex: None,
        hash: None,
        nid_hex: nid_hex.clone(),
        owner_nid_hex: owner_kind.as_ref().map(|_| nid_hex),
        owner_family: owner_kind.as_ref().map(|_| "bulk".to_string()),
        owner_kind,
        owner_image_id,
        entry_count,
        recognized_entry_count,
    }
}

fn hex_u64(value: u64) -> String {
    format!("{value:016x}")
}

#[test]
fn source_migrations_install_raw_free_omap_schema_and_targeted_indexes() {
    let conn = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_019_cephfs_journal_replay"
    );
    for table in [
        "ceph_bluestore_omap_scans",
        "ceph_bluestore_omap_scopes",
        "ceph_bluestore_rbd_directory",
        "ceph_bluestore_rbd_headers",
    ] {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query OMAP schema");
        assert!(!sql.to_ascii_lowercase().contains(" blob"));
        for forbidden in ["raw_key", "raw_value", "user_key", "entry_value"] {
            assert!(!sql.contains(forbidden), "{table} persists {forbidden}");
        }
    }
    for index in [
        "idx_ceph_bluestore_omap_scans_source",
        "idx_ceph_bluestore_omap_scopes_family",
        "idx_ceph_bluestore_omap_scopes_owner",
        "idx_ceph_bluestore_rbd_directory_image_id",
        "idx_ceph_bluestore_rbd_headers_owner",
        "idx_ceph_bluestore_objects_rbd_lookup",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
                 )",
                [index],
                |row| row.get(0),
            )
            .expect("query OMAP index");
        assert!(exists, "missing index {index}");
    }
}

#[test]
fn omap_round_trip_supports_family_owner_and_rbd_header_queries() {
    let conn = setup();
    let (rocksdb, semantic) = seed_parent(&conn, "inventory-1", "source-1");
    let expected = omap(&rocksdb, &semantic, "vm-100-disk-0", "image-100");
    let repo = CephBluestoreOmapRepo::new(&conn);
    repo.replace_for_inventory(&support::empty_omap(&rocksdb, &semantic))
        .expect("persist empty OMAP aggregate");
    repo.replace_for_inventory(&expected)
        .expect("persist OMAP aggregate");

    assert_eq!(
        repo.find_aggregate("inventory-1")
            .expect("load OMAP aggregate"),
        Some(expected.clone())
    );
    assert_eq!(
        repo.find_scopes_by_family("inventory-1", "bulk")
            .expect("query OMAP family"),
        expected.scopes.clone()
    );
    assert_eq!(
        repo.find_scopes_by_owner("inventory-1", &hex_u64(12))
            .expect("query OMAP owner"),
        vec![expected.scopes[1].clone()]
    );
    assert_eq!(
        repo.find_rbd_header("inventory-1", "image-100")
            .expect("query RBD header"),
        Some(expected.rbd_headers[0].clone())
    );
}

#[test]
fn omap_replacement_is_source_local_and_leaves_no_old_rows() {
    let conn = setup();
    let (rocksdb_1, semantic_1) = seed_parent(&conn, "inventory-1", "source-1");
    let (rocksdb_2, semantic_2) = seed_parent(&conn, "inventory-2", "source-2");
    let original_1 = omap(&rocksdb_1, &semantic_1, "old-name", "old-image");
    let expected_2 = omap(&rocksdb_2, &semantic_2, "other-name", "other-image");
    let repo = CephBluestoreOmapRepo::new(&conn);
    repo.replace_for_inventory(&original_1)
        .expect("persist source 1");
    repo.replace_for_inventory(&expected_2)
        .expect("persist source 2");

    let replacement_1 = omap(&rocksdb_1, &semantic_1, "new-name", "new-image");
    repo.replace_for_inventory(&replacement_1)
        .expect("replace source 1");

    assert_eq!(
        repo.find_aggregate("inventory-1").expect("reload source 1"),
        Some(replacement_1)
    );
    assert_eq!(
        repo.find_rbd_header("inventory-1", "old-image")
            .expect("query removed image"),
        None
    );
    assert_eq!(
        repo.find_aggregate("inventory-2").expect("reload source 2"),
        Some(expected_2)
    );
}

#[test]
fn invalid_digest_or_parent_binding_preserves_previous_snapshot() {
    let conn = setup();
    let (rocksdb, semantic) = seed_parent(&conn, "inventory-1", "source-1");
    let expected = omap(&rocksdb, &semantic, "vm-disk", "image-1");
    let repo = CephBluestoreOmapRepo::new(&conn);
    repo.replace_for_inventory(&expected)
        .expect("persist valid aggregate");

    let mut invalid_digest = expected.clone();
    invalid_digest.scan.omap_sha256 = "A".repeat(64);
    assert!(repo.replace_for_inventory(&invalid_digest).is_err());

    let mut wrong_source = expected.clone();
    wrong_source.scan.data_source_id = "source-other".to_string();
    wrong_source.scan.omap_sha256 = omap_aggregate_sha256(&wrong_source);
    assert!(repo.replace_for_inventory(&wrong_source).is_err());

    assert_eq!(
        repo.find_aggregate("inventory-1")
            .expect("reload preserved aggregate"),
        Some(expected)
    );
}

#[test]
fn deleting_osd_inventory_cascades_all_omap_rows() {
    let conn = setup();
    let (rocksdb, semantic) = seed_parent(&conn, "inventory-1", "source-1");
    let expected = omap(&rocksdb, &semantic, "vm-disk", "image-1");
    CephBluestoreOmapRepo::new(&conn)
        .replace_for_inventory(&expected)
        .expect("persist OMAP aggregate");

    conn.execute(
        "DELETE FROM ceph_osd_inventory WHERE id = ?1",
        ["inventory-1"],
    )
    .expect("delete OSD inventory");

    for table in [
        "ceph_bluestore_omap_scans",
        "ceph_bluestore_omap_scopes",
        "ceph_bluestore_rbd_directory",
        "ceph_bluestore_rbd_headers",
    ] {
        let count: u64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count cascaded rows");
        assert_eq!(count, 0, "orphan rows remain in {table}");
    }
}
