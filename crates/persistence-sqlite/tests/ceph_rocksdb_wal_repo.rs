use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluefs_replay_repo::{
            CephBluefsDirectoryRecord, CephBluefsFileRecord, CephBluefsReplayAggregate,
            CephBluefsReplayRecord,
        },
        ceph_bluefs_repo::{CephBluefsAggregate, CephBluefsSuperblockRecord},
        ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo, CephRocksdbMetadataSnapshot},
        ceph_rocksdb_repo::{
            CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
        },
        ceph_rocksdb_wal_repo::{
            CephRocksdbWalAggregate, CephRocksdbWalFileRecord, CephRocksdbWalRecord,
            CephRocksdbWalRepo,
        },
    },
    runner,
};
use rusqlite::Connection;

mod support;

const INVENTORY_ID: &str = "inventory-1";
const DATA_SOURCE_ID: &str = "source-1";
const OSD_UUID: &str = "11111111-2222-3333-4444-555555555555";

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-14T00:00:00Z')",
        [DATA_SOURCE_ID],
    )
    .expect("insert data source");
    conn
}

fn osd(description: &str) -> CephOsdInventoryRecord {
    CephOsdInventoryRecord {
        id: INVENTORY_ID.to_string(),
        data_source_id: DATA_SOURCE_ID.to_string(),
        partition_index: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        osd_uuid: OSD_UUID.to_string(),
        ceph_fsid: None,
        whoami: Some(1),
        device_role: "block".to_string(),
        device_size: 1024 * 1024,
        birth_time_seconds: 1,
        birth_time_nanoseconds: 0,
        description: description.to_string(),
        is_multi: true,
        selected_epoch: Some(1),
        valid_label_count: 1,
        label_health: "singleReplica".to_string(),
        osd_key_present: true,
        kv_backend: Some("rocksdb".to_string()),
        bluefs_enabled: Some(true),
        ceph_version_when_created: Some("19.2.3".to_string()),
        require_osd_release: Some(19),
        sanitized_metadata_json: "{}".to_string(),
    }
}

fn bluefs() -> CephBluefsAggregate {
    CephBluefsAggregate {
        superblock: CephBluefsSuperblockRecord {
            inventory_id: INVENTORY_ID.to_string(),
            data_source_id: DATA_SOURCE_ID.to_string(),
            bluefs_uuid: "22222222-3333-4444-5555-666666666666".to_string(),
            osd_uuid: OSD_UUID.to_string(),
            sequence: 1,
            block_size: 4096,
            crc32c: 1,
            struct_version: 2,
            struct_compat_version: 1,
            log_inode: 1,
            log_size: 4096,
            log_mtime_seconds: 1,
            log_mtime_nanoseconds: 0,
            log_encoding: 0,
            log_content_size: 4096,
            shared_bdev: Some(1),
            dedicated_db: Some(false),
            dedicated_wal: Some(false),
        },
        log_extents: Vec::new(),
        replay: CephBluefsReplayAggregate {
            replay: CephBluefsReplayRecord {
                inventory_id: INVENTORY_ID.to_string(),
                transaction_count: 1,
                first_sequence: 1,
                final_sequence: 1,
                logical_bytes: 4096,
                stop_reason: "boundedTail".to_string(),
            },
            directories: vec![
                CephBluefsDirectoryRecord {
                    inventory_id: INVENTORY_ID.to_string(),
                    path: "db".to_string(),
                },
                CephBluefsDirectoryRecord {
                    inventory_id: INVENTORY_ID.to_string(),
                    path: "db.wal".to_string(),
                },
            ],
            files: vec![
                bluefs_file("db/MANIFEST-000143", 2, 4096),
                bluefs_file("db.wal/000126.log", 3, 512),
                bluefs_file("db.wal/000142.log", 4, 1024),
                bluefs_file("db.wal/000143.log", 5, 0),
            ],
            file_extents: Vec::new(),
        },
    }
}

fn bluefs_file(path: &str, inode: u64, size: u64) -> CephBluefsFileRecord {
    CephBluefsFileRecord {
        inventory_id: INVENTORY_ID.to_string(),
        path: path.to_string(),
        inode,
        size,
        mtime_seconds: 1,
        mtime_nanoseconds: 0,
        encoding: 0,
        content_size: 0,
    }
}

fn rocksdb() -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: INVENTORY_ID.to_string(),
            data_source_id: DATA_SOURCE_ID.to_string(),
            active_manifest_path: "db/MANIFEST-000143".to_string(),
            identity_uuid: Some("318c61d3-7d8b-497a-b02a-d3683123595d".to_string()),
            manifest_file_number: 143,
            manifest_file_size: 4096,
            logical_edit_count: 39,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 100,
            next_file_number: 150,
            log_number: 142,
            prev_log_number: 0,
            max_column_family_id: 0,
            min_log_number_to_keep: Some(127),
        },
        column_families: vec![CephRocksdbColumnFamilyRecord {
            inventory_id: INVENTORY_ID.to_string(),
            column_family_id: 0,
            name: "default".to_string(),
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            dropped: false,
            log_number: Some(142),
        }],
        live_ssts: Vec::new(),
    }
}

fn wals() -> CephRocksdbWalAggregate {
    CephRocksdbWalAggregate {
        files: vec![
            CephRocksdbWalFileRecord {
                inventory_id: INVENTORY_ID.to_string(),
                wal_number: 142,
                bluefs_path: "db.wal/000142.log".to_string(),
                post_manifest: false,
                file_size: 1024,
                logical_record_count: 2,
                empty_batch_count: 1,
                mutation_count: 2,
                auxiliary_record_count: 3,
                logical_payload_bytes: 200,
                fragment_count: 3,
                first_sequence: Some(101),
                last_sequence: Some(103),
                first_record_offset: Some(0),
                last_record_offset: Some(128),
            },
            CephRocksdbWalFileRecord {
                inventory_id: INVENTORY_ID.to_string(),
                wal_number: 143,
                bluefs_path: "db.wal/000143.log".to_string(),
                post_manifest: false,
                file_size: 0,
                logical_record_count: 0,
                empty_batch_count: 0,
                mutation_count: 0,
                auxiliary_record_count: 0,
                logical_payload_bytes: 0,
                fragment_count: 0,
                first_sequence: None,
                last_sequence: None,
                first_record_offset: None,
                last_record_offset: None,
            },
        ],
        records: vec![
            CephRocksdbWalRecord {
                inventory_id: INVENTORY_ID.to_string(),
                wal_number: 142,
                record_ordinal: 0,
                physical_offset: 0,
                fragment_count: 1,
                recyclable_log_number: Some(142),
                batch_sequence: 101,
                mutation_count: 2,
                auxiliary_record_count: 1,
                first_mutation_sequence: Some(101),
                last_mutation_sequence: Some(102),
            },
            CephRocksdbWalRecord {
                inventory_id: INVENTORY_ID.to_string(),
                wal_number: 142,
                record_ordinal: 1,
                physical_offset: 128,
                fragment_count: 2,
                recyclable_log_number: Some(142),
                batch_sequence: 103,
                mutation_count: 0,
                auxiliary_record_count: 2,
                first_mutation_sequence: None,
                last_mutation_sequence: None,
            },
        ],
    }
}

fn persist(
    conn: &Connection,
    osd: &CephOsdInventoryRecord,
    bluefs: &CephBluefsAggregate,
    rocksdb: &CephRocksdbAggregate,
    wals: &CephRocksdbWalAggregate,
) -> persistence_sqlite::DbResult<()> {
    let latest_state = support::empty_latest_state(rocksdb);
    let semantic = support::empty_semantic(rocksdb, &latest_state);
    let omap = support::empty_omap(rocksdb, &semantic);
    CephOsdRepo::new(conn).replace_for_data_source_with_rocksdb_metadata(
        DATA_SOURCE_ID,
        std::slice::from_ref(osd),
        &[],
        CephRocksdbMetadataSnapshot {
            bluefs,
            rocksdb,
            ssts: &[],
            wals,
            latest_state: &latest_state,
            semantic: &semantic,
            omap: &omap,
        },
    )
}

#[test]
fn source_migration_installs_normalized_wal_schema_without_raw_keys_or_values() {
    let conn = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_027_artifact_keyset_indexes"
    );
    for table in ["ceph_rocksdb_wal_files", "ceph_rocksdb_wal_records"] {
        let columns = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
            ))
            .expect("prepare WAL column query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query WAL columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect WAL columns");
        assert!(!columns.iter().any(|column| {
            matches!(
                column.as_str(),
                "raw_key" | "raw_value" | "key" | "value" | "batch_payload"
            )
        }));
        if table == "ceph_rocksdb_wal_files" {
            assert!(columns.contains(&"post_manifest".to_string()));
        }
    }
}

#[test]
fn wal_file_and_logical_record_metadata_round_trip_with_empty_wal() {
    let conn = setup();
    let expected = wals();
    persist(&conn, &osd("original"), &bluefs(), &rocksdb(), &expected)
        .expect("persist WAL inventory");

    assert_eq!(
        CephRocksdbWalRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("load WAL aggregate"),
        expected
    );
}

#[test]
fn nonempty_wal_with_only_empty_batches_keeps_batch_sequence_bounds() {
    let conn = setup();
    let mut expected = wals();
    expected.records[0].mutation_count = 0;
    expected.records[0].first_mutation_sequence = None;
    expected.records[0].last_mutation_sequence = None;
    expected.records[1].batch_sequence = 101;
    expected.files[0].empty_batch_count = 2;
    expected.files[0].mutation_count = 0;
    expected.files[0].first_sequence = Some(101);
    expected.files[0].last_sequence = Some(101);

    persist(&conn, &osd("original"), &bluefs(), &rocksdb(), &expected)
        .expect("persist auxiliary-only WAL inventory");
    assert_eq!(
        CephRocksdbWalRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("load auxiliary-only WAL aggregate"),
        expected
    );
}

#[test]
fn rejects_incomplete_noncanonical_or_inconsistent_wal_metadata() {
    let conn = setup();
    let osd = osd("original");
    let bluefs = bluefs();
    let rocksdb = rocksdb();

    let mut missing_file = wals();
    missing_file.files.pop();
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &missing_file).is_err());

    let mut unbound_file = wals();
    unbound_file.files[0].wal_number = 144;
    unbound_file.files[0].bluefs_path = "db.wal/000144.log".to_string();
    unbound_file.records[0].wal_number = 144;
    unbound_file.records[1].wal_number = 144;
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &unbound_file).is_err());

    let mut noncanonical_path = wals();
    noncanonical_path.files[0].bluefs_path = "db.wal/142.log".to_string();
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &noncanonical_path).is_err());

    let mut wrong_count = wals();
    wrong_count.files[0].auxiliary_record_count += 1;
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &wrong_count).is_err());

    let mut wrong_recyclable_id = wals();
    wrong_recyclable_id.records[0].recyclable_log_number = Some(141);
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &wrong_recyclable_id).is_err());

    let mut invalid_empty = wals();
    invalid_empty.files[1].logical_payload_bytes = 1;
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &invalid_empty).is_err());
}

#[test]
fn sequence_gaps_are_accepted_but_overlapping_mutations_fail_closed() {
    let conn = setup();
    let osd = osd("original");
    let bluefs = bluefs();
    let rocksdb = rocksdb();

    let mut sequence_gap = wals();
    sequence_gap.records[0].batch_sequence = 105;
    sequence_gap.records[0].first_mutation_sequence = Some(105);
    sequence_gap.records[0].last_mutation_sequence = Some(106);
    sequence_gap.records[1].batch_sequence = 110;
    sequence_gap.files[0].first_sequence = Some(105);
    sequence_gap.files[0].last_sequence = Some(110);
    persist(&conn, &osd, &bluefs, &rocksdb, &sequence_gap)
        .expect("persist WAL inventory with sequence gaps");

    let mut overlap = wals();
    overlap.records[1].batch_sequence = 102;
    overlap.records[1].mutation_count = 1;
    overlap.records[1].first_mutation_sequence = Some(102);
    overlap.records[1].last_mutation_sequence = Some(102);
    overlap.files[0].empty_batch_count = 0;
    overlap.files[0].mutation_count = 3;
    overlap.files[0].last_sequence = Some(102);
    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &overlap).is_err());
}

#[test]
fn missing_recovery_boundary_and_missing_active_log_number_are_supported() {
    let conn = setup();
    let osd = osd("original");
    let mut missing_bluefs = bluefs();
    missing_bluefs
        .replay
        .files
        .retain(|file| file.path != "db.wal/000142.log");
    let mut missing_active = wals();
    missing_active.files.remove(0);
    missing_active.records.clear();
    persist(&conn, &osd, &missing_bluefs, &rocksdb(), &missing_active)
        .expect("persist without a physical recovery-boundary WAL");

    let mut rocksdb = rocksdb();
    rocksdb.column_families[0].log_number = None;
    rocksdb.manifest.max_column_family_id = 1;
    rocksdb.column_families.push(CephRocksdbColumnFamilyRecord {
        inventory_id: INVENTORY_ID.to_string(),
        column_family_id: 1,
        name: "dropped".to_string(),
        comparator_name: "leveldb.BytewiseComparator".to_string(),
        dropped: true,
        log_number: Some(1),
    });
    persist(&conn, &osd, &bluefs(), &rocksdb, &wals())
        .expect("resolve missing active column-family log number to zero");
}

#[test]
fn post_manifest_and_legacy_root_provenance_round_trip() {
    let conn = setup();
    let osd = osd("original");
    let rocksdb = rocksdb();

    let mut post_manifest_bluefs = bluefs();
    post_manifest_bluefs
        .replay
        .files
        .push(bluefs_file("db.wal/000150.log", 6, 0));
    let mut post_manifest = wals();
    let mut post_manifest_file = post_manifest.files[1].clone();
    post_manifest_file.wal_number = 150;
    post_manifest_file.bluefs_path = "db.wal/000150.log".to_string();
    post_manifest_file.post_manifest = true;
    post_manifest.files.push(post_manifest_file);
    persist(&conn, &osd, &post_manifest_bluefs, &rocksdb, &post_manifest)
        .expect("persist post-MANIFEST WAL");
    assert!(
        CephRocksdbWalRepo::new(&conn)
            .find_files_for_inventory(INVENTORY_ID)
            .expect("load post-MANIFEST WAL")[2]
            .post_manifest
    );

    let mut legacy_bluefs = bluefs();
    legacy_bluefs
        .replay
        .directories
        .retain(|directory| directory.path != "db.wal");
    for file in &mut legacy_bluefs.replay.files {
        if let Some(name) = file.path.strip_prefix("db.wal/") {
            file.path = format!("db/{name}");
        }
    }
    let mut legacy = wals();
    for file in &mut legacy.files {
        file.bluefs_path = file.bluefs_path.replacen("db.wal/", "db/", 1);
    }
    persist(&conn, &osd, &legacy_bluefs, &rocksdb, &legacy)
        .expect("persist legacy db-root WAL inventory");
}

#[test]
fn db_wal_directory_takes_priority_over_existing_legacy_wal_files() {
    let conn = setup();
    let osd = osd("original");
    let rocksdb = rocksdb();
    let mut bluefs = bluefs();
    bluefs
        .replay
        .files
        .push(bluefs_file("db/000142.log", 6, 1024));
    bluefs.replay.files.push(bluefs_file("db/000143.log", 7, 0));
    let mut legacy = wals();
    for file in &mut legacy.files {
        file.bluefs_path = file.bluefs_path.replacen("db.wal/", "db/", 1);
    }

    assert!(persist(&conn, &osd, &bluefs, &rocksdb, &legacy).is_err());
}

#[test]
fn recyclable_header_uses_the_low_32_bits_of_the_wal_number() {
    let conn = setup();
    let osd = osd("original");
    let rocksdb = rocksdb();
    let wal_number = (1u64 << 32) + 142;
    let wal_path = format!("db.wal/{wal_number:06}.log");
    let mut bluefs = bluefs();
    bluefs
        .replay
        .files
        .retain(|file| !file.path.starts_with("db.wal/"));
    bluefs.replay.files.push(bluefs_file(&wal_path, 6, 1024));
    let mut high_wal = wals();
    high_wal.files.truncate(1);
    high_wal.records.truncate(1);
    high_wal.files[0].wal_number = wal_number;
    high_wal.files[0].bluefs_path = wal_path;
    high_wal.files[0].post_manifest = true;
    high_wal.files[0].logical_record_count = 1;
    high_wal.files[0].empty_batch_count = 0;
    high_wal.files[0].mutation_count = 2;
    high_wal.files[0].auxiliary_record_count = 1;
    high_wal.files[0].logical_payload_bytes = 200;
    high_wal.files[0].fragment_count = 1;
    high_wal.files[0].first_sequence = Some(101);
    high_wal.files[0].last_sequence = Some(102);
    high_wal.files[0].first_record_offset = Some(0);
    high_wal.files[0].last_record_offset = Some(0);
    high_wal.records[0].wal_number = wal_number;
    high_wal.records[0].recyclable_log_number = Some(142);

    persist(&conn, &osd, &bluefs, &rocksdb, &high_wal)
        .expect("persist recyclable WAL with a 64-bit file number");
}

#[test]
fn wal_write_failure_rolls_back_the_complete_ceph_replacement() {
    let conn = setup();
    let original = wals();
    persist(&conn, &osd("original"), &bluefs(), &rocksdb(), &original)
        .expect("persist original aggregate");
    conn.execute_batch(
        "CREATE TRIGGER reject_wal_record_replacement
         BEFORE INSERT ON ceph_rocksdb_wal_records
         BEGIN
           SELECT RAISE(ABORT, 'injected WAL persistence failure');
         END;",
    )
    .expect("install WAL failure trigger");

    let result = persist(&conn, &osd("replacement"), &bluefs(), &rocksdb(), &original);

    assert!(result.is_err());
    assert_eq!(
        CephOsdRepo::new(&conn)
            .find_by_data_source(DATA_SOURCE_ID)
            .expect("reload OSD inventory")[0]
            .description,
        "original"
    );
    assert_eq!(
        CephRocksdbWalRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("reload WAL aggregate"),
        original
    );
}

#[test]
fn deleting_osd_inventory_cascades_wal_files_and_records() {
    let conn = setup();
    persist(&conn, &osd("original"), &bluefs(), &rocksdb(), &wals())
        .expect("persist WAL inventory");
    conn.execute(
        "DELETE FROM ceph_osd_inventory WHERE id = ?1",
        [INVENTORY_ID],
    )
    .expect("delete OSD inventory");

    assert_eq!(
        CephRocksdbWalRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("load cascaded WAL aggregate"),
        CephRocksdbWalAggregate {
            files: Vec::new(),
            records: Vec::new(),
        }
    );
}
