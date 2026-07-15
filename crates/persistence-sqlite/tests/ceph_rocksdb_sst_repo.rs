use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluefs_replay_repo::{
            CephBluefsDirectoryRecord, CephBluefsFileRecord, CephBluefsReplayAggregate,
            CephBluefsReplayRecord,
        },
        ceph_bluefs_repo::{CephBluefsAggregate, CephBluefsSuperblockRecord},
        ceph_bluestore_omap_repo::CephBluestoreOmapRepo,
        ceph_bluestore_semantic_repo::CephBluestoreSemanticRepo,
        ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo, CephRocksdbMetadataSnapshot},
        ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRepo,
        ceph_rocksdb_repo::{
            CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
            CephRocksdbManifestRecord,
        },
        ceph_rocksdb_sst_repo::{CephRocksdbSstRecord, CephRocksdbSstRepo},
        ceph_rocksdb_wal_repo::{
            CephRocksdbWalAggregate, CephRocksdbWalFileRecord, CephRocksdbWalRecord,
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

fn osd() -> CephOsdInventoryRecord {
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
        description: "BlueStore OSD".to_string(),
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
    bluefs_with_file_number(146)
}

fn bluefs_with_file_number(file_number: u64) -> CephBluefsAggregate {
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
                CephBluefsFileRecord {
                    inventory_id: INVENTORY_ID.to_string(),
                    path: format!("db/{file_number:06}.sst"),
                    inode: 2,
                    size: 8192,
                    mtime_seconds: 1,
                    mtime_nanoseconds: 0,
                    encoding: 0,
                    content_size: 8192,
                },
                CephBluefsFileRecord {
                    inventory_id: INVENTORY_ID.to_string(),
                    path: "db.wal/000127.log".to_string(),
                    inode: 3,
                    size: 1024,
                    mtime_seconds: 1,
                    mtime_nanoseconds: 0,
                    encoding: 0,
                    content_size: 1024,
                },
            ],
            file_extents: Vec::new(),
        },
    }
}

fn rocksdb() -> CephRocksdbAggregate {
    rocksdb_with_file_number(146)
}

fn rocksdb_with_file_number(file_number: u64) -> CephRocksdbAggregate {
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
            last_sequence: 1_077_117,
            next_file_number: file_number + 2,
            log_number: 127,
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
            log_number: Some(127),
        }],
        live_ssts: vec![CephRocksdbLiveSstRecord {
            inventory_id: INVENTORY_ID.to_string(),
            column_family_id: 0,
            level: 0,
            file_number,
            path_id: 0,
            format: "newFile4".to_string(),
            file_size: 8192,
            smallest_sequence: Some(1),
            largest_sequence: Some(100),
            smallest_internal_key_length: 9,
            largest_internal_key_length: 10,
        }],
    }
}

fn sst() -> CephRocksdbSstRecord {
    sst_with_file_number(146)
}

fn sst_with_file_number(file_number: u64) -> CephRocksdbSstRecord {
    CephRocksdbSstRecord {
        inventory_id: INVENTORY_ID.to_string(),
        file_number,
        column_family_id: 0,
        level: 0,
        bluefs_path: format!("db/{file_number:06}.sst"),
        file_size: 8192,
        table_magic_hex: "88e241b785f4cff7".to_string(),
        format_version: 5,
        checksum_type: "xxh3".to_string(),
        metaindex_offset: 7000,
        metaindex_size: 128,
        index_offset: 7200,
        index_size: 256,
        data_block_count: 148,
        entry_count: 23_364,
        deletion_count: 0,
        merge_operand_count: 0,
        range_deletion_count: 0,
        raw_key_size: 420_609,
        raw_value_size: 298_145,
        data_size: 6999,
        properties_index_size: 256,
        filter_size: 0,
        compression_name: "LZ4".to_string(),
        comparator_name: "leveldb.BytewiseComparator".to_string(),
        column_family_name: "default".to_string(),
        original_file_number: file_number,
        db_identity: Some("318c61d3-7d8b-497a-b02a-d3683123595d".to_string()),
        db_session_identity: Some("session-1".to_string()),
        key_space_summary_version: 1,
        key_space_summary_json:
            r#"{"version":1,"complete":true,"scannedEntries":23364,"scannedDecompressedBytes":1048576,"buckets":[{"name":"unknown","count":23364,"minKeyLength":1,"maxKeyLength":64}]}"#
                .to_string(),
        scan_complete: true,
    }
}

fn wals(rocksdb: &CephRocksdbAggregate) -> CephRocksdbWalAggregate {
    let first_sequence = rocksdb.manifest.last_sequence + 1;
    CephRocksdbWalAggregate {
        files: vec![CephRocksdbWalFileRecord {
            inventory_id: INVENTORY_ID.to_string(),
            wal_number: 127,
            bluefs_path: "db.wal/000127.log".to_string(),
            post_manifest: false,
            file_size: 1024,
            logical_record_count: 1,
            empty_batch_count: 0,
            mutation_count: 1,
            auxiliary_record_count: 0,
            logical_payload_bytes: 32,
            fragment_count: 1,
            first_sequence: Some(first_sequence),
            last_sequence: Some(first_sequence),
            first_record_offset: Some(0),
            last_record_offset: Some(0),
        }],
        records: vec![CephRocksdbWalRecord {
            inventory_id: INVENTORY_ID.to_string(),
            wal_number: 127,
            record_ordinal: 0,
            physical_offset: 0,
            fragment_count: 1,
            recyclable_log_number: Some(127),
            batch_sequence: first_sequence,
            mutation_count: 1,
            auxiliary_record_count: 0,
            first_mutation_sequence: Some(first_sequence),
            last_mutation_sequence: Some(first_sequence),
        }],
    }
}

fn persist(conn: &Connection, ssts: &[CephRocksdbSstRecord]) -> persistence_sqlite::DbResult<()> {
    persist_with_file_number(conn, ssts, 146)
}

fn persist_with_file_number(
    conn: &Connection,
    ssts: &[CephRocksdbSstRecord],
    file_number: u64,
) -> persistence_sqlite::DbResult<()> {
    let osd = osd();
    let bluefs = bluefs_with_file_number(file_number);
    let rocksdb = rocksdb_with_file_number(file_number);
    let latest_state = support::empty_latest_state(&rocksdb);
    let semantic = support::empty_semantic(&rocksdb, &latest_state);
    let omap = support::empty_omap(&rocksdb, &semantic);
    CephOsdRepo::new(conn).replace_for_data_source_with_rocksdb_metadata(
        DATA_SOURCE_ID,
        std::slice::from_ref(&osd),
        &[],
        CephRocksdbMetadataSnapshot {
            bluefs: &bluefs,
            rocksdb: &rocksdb,
            ssts,
            wals: &wals(&rocksdb),
            latest_state: &latest_state,
            semantic: &semantic,
            omap: &omap,
        },
    )
}

#[test]
fn source_migration_installs_sst_inventory_without_raw_key_or_value_columns() {
    let conn = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_014_ceph_osd_device_bindings"
    );
    let mut statement = conn
        .prepare("SELECT name FROM pragma_table_info('ceph_rocksdb_sst_inventory') ORDER BY cid")
        .expect("prepare column query");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    assert!(columns.contains(&"table_magic_hex".to_string()));
    assert!(columns.contains(&"key_space_summary_json".to_string()));
    assert!(!columns.iter().any(|column| {
        matches!(
            column.as_str(),
            "raw_key" | "raw_value" | "smallest_key" | "largest_key"
        )
    }));
}

#[test]
fn complete_live_sst_inventory_round_trips() {
    let conn = setup();
    let expected = sst();
    persist(&conn, std::slice::from_ref(&expected)).expect("persist SST inventory");

    assert_eq!(
        CephRocksdbSstRepo::new(&conn)
            .find_for_inventory(INVENTORY_ID)
            .expect("load SST inventory"),
        vec![expected]
    );
}

#[test]
fn seven_digit_file_number_round_trips_without_path_truncation() {
    let conn = setup();
    let expected = sst_with_file_number(1_000_000);
    persist_with_file_number(&conn, std::slice::from_ref(&expected), 1_000_000)
        .expect("persist seven-digit SST inventory");

    assert_eq!(
        CephRocksdbSstRepo::new(&conn)
            .find_for_inventory(INVENTORY_ID)
            .expect("load seven-digit SST inventory"),
        vec![expected]
    );
}

#[test]
fn replacement_requires_complete_live_set_and_rolls_back_previous_records() {
    let conn = setup();
    let expected = sst();
    persist(&conn, std::slice::from_ref(&expected)).expect("persist SST inventory");

    assert!(persist(&conn, &[]).is_err());
    assert_eq!(
        CephRocksdbSstRepo::new(&conn)
            .find_for_inventory(INVENTORY_ID)
            .expect("reload SST inventory"),
        vec![expected]
    );
}

#[test]
fn rejects_manifest_identity_and_structure_mismatches() {
    let conn = setup();

    let mut wrong_db_identity = sst();
    wrong_db_identity.db_identity = Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string());
    assert!(persist(&conn, &[wrong_db_identity]).is_err());

    let mut wrong_path = sst();
    wrong_path.bluefs_path = "db/000147.sst".to_string();
    assert!(persist(&conn, &[wrong_path]).is_err());

    let mut wrong_size = sst();
    wrong_size.file_size += 1;
    assert!(persist(&conn, &[wrong_size]).is_err());

    let mut wrong_magic = sst();
    wrong_magic.table_magic_hex = "0000000000000000".to_string();
    assert!(persist(&conn, &[wrong_magic]).is_err());

    let mut bad_json = sst();
    bad_json.key_space_summary_json = "[]".to_string();
    assert!(persist(&conn, &[bad_json]).is_err());

    let mut incomplete = sst();
    incomplete.key_space_summary_json = incomplete
        .key_space_summary_json
        .replace(r#""complete":true"#, r#""complete":false"#);
    assert!(persist(&conn, &[incomplete]).is_err());

    for field in ["raw_key", "userKey", "keyHex", "RawKey", "unexpected"] {
        let mut unknown_root_field = sst();
        unknown_root_field.key_space_summary_json =
            unknown_root_field.key_space_summary_json.replacen(
                r#""buckets":"#,
                &format!(r#""{field}":"forbidden","buckets":"#),
                1,
            );
        assert!(persist(&conn, &[unknown_root_field]).is_err());
    }

    for field in ["raw_key", "userKey", "keyHex", "RawKey", "unexpected"] {
        let mut unknown_bucket_field = sst();
        unknown_bucket_field.key_space_summary_json =
            unknown_bucket_field.key_space_summary_json.replacen(
                r#""maxKeyLength":64"#,
                &format!(r#""maxKeyLength":64,"{field}":"forbidden""#),
                1,
            );
        assert!(persist(&conn, &[unknown_bucket_field]).is_err());
    }

    let mut inconsistent_count = sst();
    inconsistent_count.key_space_summary_json = inconsistent_count
        .key_space_summary_json
        .replace(r#""scannedEntries":23364"#, r#""scannedEntries":23365"#);
    assert!(persist(&conn, &[inconsistent_count]).is_err());

    let mut consistently_wrong_census = sst();
    consistently_wrong_census.key_space_summary_json = consistently_wrong_census
        .key_space_summary_json
        .replace(r#""scannedEntries":23364"#, r#""scannedEntries":23363"#)
        .replace(r#""count":23364"#, r#""count":23363"#);
    assert!(persist(&conn, &[consistently_wrong_census]).is_err());

    let mut unknown_compression = sst();
    unknown_compression.compression_name = "FutureCompression".to_string();
    assert!(persist(&conn, &[unknown_compression]).is_err());
}

#[test]
fn sqlite_write_failure_rolls_back_the_complete_ceph_aggregate() {
    let conn = setup();
    let expected = sst();
    persist(&conn, std::slice::from_ref(&expected)).expect("persist initial SST inventory");
    conn.execute_batch(
        "CREATE TRIGGER reject_sst_replacement
         BEFORE INSERT ON ceph_rocksdb_sst_inventory
         BEGIN
           SELECT RAISE(ABORT, 'injected SST persistence failure');
         END;",
    )
    .expect("install failure trigger");

    let mut changed_osd = osd();
    changed_osd.description = "must roll back".to_string();
    let bluefs = bluefs();
    let rocksdb = rocksdb();
    let latest_state = support::empty_latest_state(&rocksdb);
    let semantic = support::empty_semantic(&rocksdb, &latest_state);
    let omap = support::empty_omap(&rocksdb, &semantic);
    let result = CephOsdRepo::new(&conn).replace_for_data_source_with_rocksdb_metadata(
        DATA_SOURCE_ID,
        std::slice::from_ref(&changed_osd),
        &[],
        CephRocksdbMetadataSnapshot {
            bluefs: &bluefs,
            rocksdb: &rocksdb,
            ssts: std::slice::from_ref(&expected),
            wals: &wals(&rocksdb),
            latest_state: &latest_state,
            semantic: &semantic,
            omap: &omap,
        },
    );

    assert!(result.is_err());
    assert_eq!(
        CephOsdRepo::new(&conn)
            .find_by_data_source(DATA_SOURCE_ID)
            .expect("reload OSD aggregate")[0]
            .description,
        "BlueStore OSD"
    );
    assert_eq!(
        CephRocksdbSstRepo::new(&conn)
            .find_for_inventory(INVENTORY_ID)
            .expect("reload SST inventory"),
        vec![expected]
    );
}

#[test]
fn final_semantic_write_failure_rolls_back_latest_state_and_osd_aggregate() {
    let conn = setup();
    let expected_sst = sst();
    persist(&conn, std::slice::from_ref(&expected_sst)).expect("persist initial aggregate");
    let expected_latest_state = CephRocksdbLatestStateRepo::new(&conn)
        .find(INVENTORY_ID)
        .expect("load initial latest state");
    let expected_semantic = CephBluestoreSemanticRepo::new(&conn)
        .find_aggregate(INVENTORY_ID)
        .expect("load initial semantics");
    conn.execute_batch(
        "CREATE TRIGGER reject_final_semantic_replacement
         BEFORE INSERT ON ceph_bluestore_semantic_scans
         BEGIN
           SELECT RAISE(ABORT, 'injected final semantic persistence failure');
         END;",
    )
    .expect("install semantic failure trigger");

    let mut changed_osd = osd();
    changed_osd.description = "must roll back at semantic stage".to_string();
    let bluefs = bluefs();
    let rocksdb = rocksdb();
    let mut latest_state = support::empty_latest_state(&rocksdb);
    latest_state[0].latest_state_sha256 = "e".repeat(64);
    let semantic = support::empty_semantic(&rocksdb, &latest_state);
    let omap = support::empty_omap(&rocksdb, &semantic);
    let result = CephOsdRepo::new(&conn).replace_for_data_source_with_rocksdb_metadata(
        DATA_SOURCE_ID,
        std::slice::from_ref(&changed_osd),
        &[],
        CephRocksdbMetadataSnapshot {
            bluefs: &bluefs,
            rocksdb: &rocksdb,
            ssts: std::slice::from_ref(&expected_sst),
            wals: &wals(&rocksdb),
            latest_state: &latest_state,
            semantic: &semantic,
            omap: &omap,
        },
    );

    assert!(result.is_err());
    assert_eq!(
        CephOsdRepo::new(&conn)
            .find_by_data_source(DATA_SOURCE_ID)
            .expect("reload OSD aggregate")[0]
            .description,
        "BlueStore OSD"
    );
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find(INVENTORY_ID)
            .expect("reload latest state"),
        expected_latest_state
    );
    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("reload semantics"),
        expected_semantic
    );
}

#[test]
fn final_omap_write_failure_rolls_back_semantics_latest_state_and_osd_aggregate() {
    let conn = setup();
    let expected_sst = sst();
    persist(&conn, std::slice::from_ref(&expected_sst)).expect("persist initial aggregate");
    let expected_latest_state = CephRocksdbLatestStateRepo::new(&conn)
        .find(INVENTORY_ID)
        .expect("load initial latest state");
    let expected_semantic = CephBluestoreSemanticRepo::new(&conn)
        .find_aggregate(INVENTORY_ID)
        .expect("load initial semantics");
    let expected_omap = CephBluestoreOmapRepo::new(&conn)
        .find_aggregate(INVENTORY_ID)
        .expect("load initial OMAP");
    conn.execute_batch(
        "CREATE TRIGGER reject_final_omap_replacement
         BEFORE INSERT ON ceph_bluestore_omap_scans
         BEGIN
           SELECT RAISE(ABORT, 'injected final OMAP persistence failure');
         END;",
    )
    .expect("install OMAP failure trigger");

    let mut changed_osd = osd();
    changed_osd.description = "must roll back at OMAP stage".to_string();
    let bluefs = bluefs();
    let rocksdb = rocksdb();
    let mut latest_state = support::empty_latest_state(&rocksdb);
    latest_state[0].latest_state_sha256 = "e".repeat(64);
    let semantic = support::empty_semantic(&rocksdb, &latest_state);
    let omap = support::empty_omap(&rocksdb, &semantic);
    let result = CephOsdRepo::new(&conn).replace_for_data_source_with_rocksdb_metadata(
        DATA_SOURCE_ID,
        std::slice::from_ref(&changed_osd),
        &[],
        CephRocksdbMetadataSnapshot {
            bluefs: &bluefs,
            rocksdb: &rocksdb,
            ssts: std::slice::from_ref(&expected_sst),
            wals: &wals(&rocksdb),
            latest_state: &latest_state,
            semantic: &semantic,
            omap: &omap,
        },
    );

    assert!(result.is_err());
    assert_eq!(
        CephOsdRepo::new(&conn)
            .find_by_data_source(DATA_SOURCE_ID)
            .expect("reload OSD aggregate")[0]
            .description,
        "BlueStore OSD"
    );
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find(INVENTORY_ID)
            .expect("reload latest state"),
        expected_latest_state
    );
    assert_eq!(
        CephBluestoreSemanticRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("reload semantics"),
        expected_semantic
    );
    assert_eq!(
        CephBluestoreOmapRepo::new(&conn)
            .find_aggregate(INVENTORY_ID)
            .expect("reload OMAP"),
        expected_omap
    );
}
