use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluefs_replay_repo::{
            CephBluefsDirectoryRecord, CephBluefsFileRecord, CephBluefsReplayAggregate,
            CephBluefsReplayRecord,
        },
        ceph_bluefs_repo::{CephBluefsAggregate, CephBluefsSuperblockRecord},
        ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
        ceph_rocksdb_repo::{
            CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
            CephRocksdbManifestRecord, CephRocksdbRepo,
        },
    },
    runner,
};
use rusqlite::Connection;

fn setup_source_db() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    for data_source_id in ["source-1", "source-2"] {
        conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, imported_at
             ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-13T00:00:00Z')",
            [data_source_id],
        )
        .expect("insert source metadata");
    }
    conn
}

fn osd(inventory_id: &str, data_source_id: &str, osd_uuid: &str) -> CephOsdInventoryRecord {
    CephOsdInventoryRecord {
        id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        partition_index: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        osd_uuid: osd_uuid.to_string(),
        ceph_fsid: Some("11111111-2222-3333-4444-555555555555".to_string()),
        whoami: Some(1),
        device_role: "block".to_string(),
        device_size: 8 * 1024 * 1024 * 1024,
        birth_time_seconds: 1_700_000_000,
        birth_time_nanoseconds: 123,
        description: "BlueStore OSD".to_string(),
        is_multi: true,
        selected_epoch: Some(42),
        valid_label_count: 2,
        label_health: "healthy".to_string(),
        osd_key_present: true,
        kv_backend: Some("rocksdb".to_string()),
        bluefs_enabled: Some(true),
        ceph_version_when_created: Some("ceph version 19.2.3".to_string()),
        require_osd_release: Some(19),
        sanitized_metadata_json: r#"{"bluefs":"1","osd_key_present":true}"#.to_string(),
    }
}

fn bluefs(
    inventory_id: &str,
    data_source_id: &str,
    osd_uuid: &str,
    sequence: u64,
) -> CephBluefsAggregate {
    CephBluefsAggregate {
        superblock: CephBluefsSuperblockRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            bluefs_uuid: format!("bluefs-{inventory_id}"),
            osd_uuid: osd_uuid.to_string(),
            sequence,
            block_size: 4096,
            crc32c: 0x1234_5678,
            struct_version: 2,
            struct_compat_version: 1,
            log_inode: 1,
            log_size: 64 * 1024,
            log_mtime_seconds: 1_700_000_000,
            log_mtime_nanoseconds: 123,
            log_encoding: 0,
            log_content_size: 64 * 1024,
            shared_bdev: Some(1),
            dedicated_db: Some(false),
            dedicated_wal: Some(false),
        },
        log_extents: Vec::new(),
        replay: CephBluefsReplayAggregate {
            replay: CephBluefsReplayRecord {
                inventory_id: inventory_id.to_string(),
                transaction_count: 4,
                first_sequence: 1,
                final_sequence: sequence,
                logical_bytes: 0x22_000,
                stop_reason: "invalidTail".to_string(),
            },
            directories: vec![CephBluefsDirectoryRecord {
                inventory_id: inventory_id.to_string(),
                path: "db".to_string(),
            }],
            files: vec![
                bluefs_file(inventory_id, "db/CURRENT", 2, 16),
                bluefs_file(inventory_id, "db/MANIFEST-000143", 3, 4096),
            ],
            file_extents: Vec::new(),
        },
    }
}

fn bluefs_file(inventory_id: &str, path: &str, inode: u64, size: u64) -> CephBluefsFileRecord {
    CephBluefsFileRecord {
        inventory_id: inventory_id.to_string(),
        path: path.to_string(),
        inode,
        size,
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 123,
        encoding: 0,
        content_size: size,
    }
}

fn rocksdb(inventory_id: &str, data_source_id: &str, manifest_number: u64) -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            active_manifest_path: format!("db/MANIFEST-{manifest_number:06}"),
            identity_uuid: Some("318c61d3-7d8b-497a-b02a-d3683123595d".to_string()),
            manifest_file_number: manifest_number,
            manifest_file_size: 4096,
            logical_edit_count: 39,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 1_077_117,
            next_file_number: manifest_number + 5,
            log_number: 142,
            prev_log_number: 0,
            max_column_family_id: 2,
            min_log_number_to_keep: Some(127),
        },
        column_families: vec![
            column_family(inventory_id, 0, "default", false),
            column_family(inventory_id, 1, "m-0", false),
            column_family(inventory_id, 2, "legacy", true),
        ],
        live_ssts: vec![
            live_sst(inventory_id, 0, 0, manifest_number + 1),
            live_sst(inventory_id, 1, 2, manifest_number + 2),
        ],
    }
}

fn column_family(
    inventory_id: &str,
    column_family_id: u32,
    name: &str,
    dropped: bool,
) -> CephRocksdbColumnFamilyRecord {
    CephRocksdbColumnFamilyRecord {
        inventory_id: inventory_id.to_string(),
        column_family_id,
        name: name.to_string(),
        comparator_name: "leveldb.BytewiseComparator".to_string(),
        dropped,
    }
}

fn live_sst(
    inventory_id: &str,
    column_family_id: u32,
    level: u32,
    file_number: u64,
) -> CephRocksdbLiveSstRecord {
    CephRocksdbLiveSstRecord {
        inventory_id: inventory_id.to_string(),
        column_family_id,
        level,
        file_number,
        path_id: 0,
        format: "newFile4".to_string(),
        file_size: 8192,
        smallest_sequence: Some(100),
        largest_sequence: Some(200),
        smallest_internal_key_length: 17,
        largest_internal_key_length: 29,
    }
}

fn persist(
    conn: &Connection,
    osd: &CephOsdInventoryRecord,
    bluefs: &CephBluefsAggregate,
    rocksdb: Option<&CephRocksdbAggregate>,
) -> persistence_sqlite::DbResult<()> {
    CephOsdRepo::new(conn).replace_for_data_source_with_metadata(
        &osd.data_source_id,
        std::slice::from_ref(osd),
        &[],
        Some(bluefs),
        rocksdb,
    )
}

#[test]
fn source_migration_installs_control_plane_schema_without_plaintext_internal_keys() {
    let conn = setup_source_db();

    assert_eq!(
        runner::latest_source_version(),
        "source_008_ceph_rocksdb_inventory"
    );
    for table in [
        "ceph_rocksdb_manifests",
        "ceph_rocksdb_column_families",
        "ceph_rocksdb_live_files",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query RocksDB table");
        assert!(exists, "missing table {table}");
    }

    let mut statement = conn
        .prepare("SELECT name FROM pragma_table_info('ceph_rocksdb_live_files') ORDER BY cid")
        .expect("prepare column query");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    assert!(columns.contains(&"smallest_internal_key_length".to_string()));
    assert!(columns.contains(&"largest_internal_key_length".to_string()));
    assert!(!columns.contains(&"smallest_internal_key".to_string()));
    assert!(!columns.contains(&"largest_internal_key".to_string()));
}

#[test]
fn rocksdb_control_plane_inventory_round_trips_in_stable_order() {
    let conn = setup_source_db();
    let osd = osd("inventory-1", "source-1", "osd-1");
    let bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);
    let expected = rocksdb("inventory-1", "source-1", 143);

    persist(&conn, &osd, &bluefs, Some(&expected)).expect("persist RocksDB inventory");

    let repo = CephRocksdbRepo::new(&conn);
    assert_eq!(
        repo.find_aggregate("inventory-1").expect("load aggregate"),
        Some(expected.clone())
    );
    assert_eq!(
        repo.find_by_data_source("source-1")
            .expect("load source manifests"),
        vec![expected.manifest]
    );
}

#[test]
fn replacement_is_source_local_and_allows_same_file_numbers() {
    let conn = setup_source_db();
    let osd_1 = osd("inventory-1", "source-1", "osd-1");
    let bluefs_1 = bluefs("inventory-1", "source-1", "osd-1", 50);
    let original_1 = rocksdb("inventory-1", "source-1", 143);
    let osd_2 = osd("inventory-2", "source-2", "osd-2");
    let bluefs_2 = bluefs("inventory-2", "source-2", "osd-2", 60);
    let expected_2 = rocksdb("inventory-2", "source-2", 143);
    persist(&conn, &osd_1, &bluefs_1, Some(&original_1)).expect("persist source 1");
    persist(&conn, &osd_2, &bluefs_2, Some(&expected_2)).expect("persist source 2");

    let replacement_1 = rocksdb("inventory-1", "source-1", 150);
    persist(&conn, &osd_1, &bluefs_1, Some(&replacement_1)).expect("replace source 1");

    let repo = CephRocksdbRepo::new(&conn);
    assert_eq!(
        repo.find_aggregate("inventory-1").unwrap(),
        Some(replacement_1)
    );
    assert_eq!(
        repo.find_aggregate("inventory-2").unwrap(),
        Some(expected_2)
    );
}

#[test]
fn rocksdb_none_clears_the_previous_snapshot() {
    let conn = setup_source_db();
    let osd = osd("inventory-1", "source-1", "osd-1");
    let bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);
    let records = rocksdb("inventory-1", "source-1", 143);
    persist(&conn, &osd, &bluefs, Some(&records)).expect("persist RocksDB inventory");

    persist(&conn, &osd, &bluefs, None).expect("replace without RocksDB inventory");

    let repo = CephRocksdbRepo::new(&conn);
    assert_eq!(repo.find_aggregate("inventory-1").unwrap(), None);
    assert!(repo.find_column_families("inventory-1").unwrap().is_empty());
    assert!(repo.find_live_ssts("inventory-1").unwrap().is_empty());
}

#[test]
fn deleting_osd_inventory_cascades_the_entire_rocksdb_snapshot() {
    let conn = setup_source_db();
    let osd = osd("inventory-1", "source-1", "osd-1");
    let bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);
    let records = rocksdb("inventory-1", "source-1", 143);
    persist(&conn, &osd, &bluefs, Some(&records)).expect("persist RocksDB inventory");

    conn.execute(
        "DELETE FROM ceph_osd_inventory WHERE id = ?1",
        ["inventory-1"],
    )
    .expect("delete OSD inventory");

    let repo = CephRocksdbRepo::new(&conn);
    assert_eq!(repo.find_aggregate("inventory-1").unwrap(), None);
    for table in [
        "ceph_rocksdb_manifests",
        "ceph_rocksdb_column_families",
        "ceph_rocksdb_live_files",
    ] {
        let count: u32 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count cascaded rows");
        assert_eq!(count, 0, "orphan rows remain in {table}");
    }
}

#[test]
fn failed_rocksdb_insert_rolls_back_osd_bluefs_and_previous_snapshot() {
    let conn = setup_source_db();
    let original_osd = osd("inventory-1", "source-1", "osd-1");
    let original_bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);
    let original_rocksdb = rocksdb("inventory-1", "source-1", 143);
    persist(
        &conn,
        &original_osd,
        &original_bluefs,
        Some(&original_rocksdb),
    )
    .expect("persist original aggregate");

    let mut replacement_osd = original_osd.clone();
    replacement_osd.description = "replacement".to_string();
    let replacement_bluefs = bluefs("inventory-1", "source-1", "osd-1", 51);
    let mut invalid_rocksdb = rocksdb("inventory-1", "source-1", 150);
    let sqlite_overflow = i64::MAX as u64 + 1;
    invalid_rocksdb.manifest.active_manifest_path = format!("db/MANIFEST-{sqlite_overflow}");
    invalid_rocksdb.manifest.manifest_file_number = sqlite_overflow;
    invalid_rocksdb.manifest.next_file_number = sqlite_overflow + 1;
    let result = persist(
        &conn,
        &replacement_osd,
        &replacement_bluefs,
        Some(&invalid_rocksdb),
    );

    assert!(result.is_err());
    assert_eq!(
        CephOsdRepo::new(&conn)
            .find_by_data_source("source-1")
            .expect("reload OSD inventory"),
        vec![original_osd]
    );
    assert_eq!(
        persistence_sqlite::repositories::ceph_bluefs_repo::CephBluefsRepo::new(&conn)
            .find_by_data_source("source-1")
            .expect("reload BlueFS inventory"),
        vec![original_bluefs.superblock]
    );
    assert_eq!(
        CephRocksdbRepo::new(&conn)
            .find_aggregate("inventory-1")
            .expect("reload RocksDB inventory"),
        Some(original_rocksdb)
    );
}

#[test]
fn rejects_live_files_outside_the_recovered_file_number_boundary() {
    let conn = setup_source_db();
    let osd = osd("inventory-1", "source-1", "osd-1");
    let bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);
    let mut invalid = rocksdb("inventory-1", "source-1", 143);
    invalid.live_ssts[0].file_number = invalid.manifest.next_file_number;

    assert!(persist(&conn, &osd, &bluefs, Some(&invalid)).is_err());
}

#[test]
fn rejects_live_files_with_inconsistent_format_metadata() {
    let conn = setup_source_db();
    let osd = osd("inventory-1", "source-1", "osd-1");
    let bluefs = bluefs("inventory-1", "source-1", "osd-1", 50);

    let mut missing_sequences = rocksdb("inventory-1", "source-1", 143);
    missing_sequences.live_ssts[0].smallest_sequence = None;
    missing_sequences.live_ssts[0].largest_sequence = None;
    assert!(persist(&conn, &osd, &bluefs, Some(&missing_sequences)).is_err());

    let mut legacy_with_sequences = rocksdb("inventory-1", "source-1", 143);
    legacy_with_sequences.live_ssts[0].format = "newFile".to_string();
    assert!(persist(&conn, &osd, &bluefs, Some(&legacy_with_sequences)).is_err());

    let mut short_internal_key = rocksdb("inventory-1", "source-1", 143);
    short_internal_key.live_ssts[0].smallest_internal_key_length = 7;
    assert!(persist(&conn, &osd, &bluefs, Some(&short_internal_key)).is_err());
}
