use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluestore_semantic_repo::latest_state_set_sha256,
        ceph_rocksdb_latest_state_repo::{
            validate_replacement, CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo,
        },
        ceph_rocksdb_repo::{
            CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
        },
    },
    runner,
};
use rusqlite::Connection;

const MAX_SEQUENCE: u64 = (1u64 << 56) - 1;

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    conn
}

fn seed_control_plane(
    conn: &Connection,
    inventory_id: &str,
    data_source_id: &str,
) -> CephRocksdbAggregate {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-14T00:00:00Z')",
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

    let aggregate = rocksdb(inventory_id, data_source_id);
    let manifest = &aggregate.manifest;
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, identity_uuid,
            manifest_file_number, manifest_file_size, logical_edit_count,
            comparator_name, last_sequence, next_file_number, log_number,
            prev_log_number, max_column_family_id, min_log_number_to_keep
         ) VALUES (
            ?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
         )",
        rusqlite::params![
            manifest.inventory_id,
            manifest.data_source_id,
            manifest.active_manifest_path,
            manifest.manifest_file_number,
            manifest.manifest_file_size,
            manifest.logical_edit_count,
            manifest.comparator_name,
            manifest.last_sequence,
            manifest.next_file_number,
            manifest.log_number,
            manifest.prev_log_number,
            manifest.max_column_family_id,
            manifest.min_log_number_to_keep,
        ],
    )
    .expect("insert RocksDB manifest");
    for column_family in &aggregate.column_families {
        conn.execute(
            "INSERT INTO ceph_rocksdb_column_families (
                inventory_id, column_family_id, name, comparator_name, dropped, log_number
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                column_family.inventory_id,
                column_family.column_family_id,
                column_family.name,
                column_family.comparator_name,
                column_family.dropped,
                column_family.log_number,
            ],
        )
        .expect("insert column family");
    }
    aggregate
}

fn rocksdb(inventory_id: &str, data_source_id: &str) -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            active_manifest_path: "db/MANIFEST-000143".to_string(),
            identity_uuid: None,
            manifest_file_number: 143,
            manifest_file_size: 4096,
            logical_edit_count: 10,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 100,
            next_file_number: 150,
            log_number: 142,
            prev_log_number: 0,
            max_column_family_id: 2,
            min_log_number_to_keep: Some(120),
        },
        column_families: vec![
            column_family(inventory_id, 0, "default", false),
            column_family(inventory_id, 1, "m-0", false),
            column_family(inventory_id, 2, "legacy", true),
        ],
        live_ssts: Vec::new(),
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
        log_number: Some(142),
    }
}

fn records(inventory_id: &str) -> Vec<CephRocksdbLatestStateRecord> {
    vec![
        record(inventory_id, 1, "m-0", '4'),
        record(inventory_id, 0, "default", '1'),
    ]
}

fn record(
    inventory_id: &str,
    column_family_id: u32,
    column_family_name: &str,
    digest: char,
) -> CephRocksdbLatestStateRecord {
    CephRocksdbLatestStateRecord {
        inventory_id: inventory_id.to_string(),
        column_family_id,
        column_family_name: column_family_name.to_string(),
        schema_version: 1,
        sharding_sha256: "a".repeat(64),
        point_mutation_count: 8,
        sst_point_mutation_count: 6,
        wal_point_mutation_count: 2,
        range_mutation_count: 2,
        sst_range_mutation_count: 1,
        wal_range_mutation_count: 1,
        latest_value_count: 3,
        deleted_key_count: 2,
        delete_decision_count: 1,
        single_delete_decision_count: 0,
        range_delete_decision_count: 1,
        merge_resolved_count: 1,
        merge_operand_count: 2,
        range_hidden_version_count: 1,
        smallest_sequence: Some(1),
        largest_sequence: Some(10),
        point_sha256: digest.to_string().repeat(64),
        range_sha256: "b".repeat(64),
        latest_state_sha256: "c".repeat(64),
        scan_complete: true,
    }
}

#[test]
fn source_011_installs_digest_only_latest_state_schema() {
    let conn = setup();

    assert_eq!(
        runner::latest_source_version(),
        "source_021_cephfs_assembly_capability"
    );
    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('ceph_rocksdb_latest_state') ORDER BY cid")
        .expect("prepare columns")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    for required in [
        "sharding_sha256",
        "point_sha256",
        "range_sha256",
        "latest_state_sha256",
        "latest_value_count",
        "deleted_key_count",
    ] {
        assert!(
            columns.contains(&required.to_string()),
            "missing {required}"
        );
    }
    assert!(
        columns
            .iter()
            .all(|column| !matches!(column.as_str(), "key" | "value" | "raw_key" | "raw_value")),
        "latest-state schema persists raw key/value material"
    );
}

#[test]
fn latest_state_round_trips_in_cf_order_and_replaces_source_locally() {
    let conn = setup();
    let rocksdb_1 = seed_control_plane(&conn, "inventory-1", "source-1");
    let rocksdb_2 = seed_control_plane(&conn, "inventory-2", "source-2");
    let original_1 = records("inventory-1");
    let original_2 = records("inventory-2");
    validate_replacement(&rocksdb_1, &original_1).expect("validate source 1");
    validate_replacement(&rocksdb_2, &original_2).expect("validate source 2");
    CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &original_1)
        .expect("insert source 1");
    CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-2", &original_2)
        .expect("insert source 2");

    let expected_1 = vec![original_1[1].clone(), original_1[0].clone()];
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find("inventory-1")
            .expect("find source 1"),
        expected_1
    );

    let mut replacement_1 = original_1.clone();
    replacement_1[0].latest_state_sha256 = "d".repeat(64);
    replacement_1[1].latest_state_sha256 = "e".repeat(64);
    conn.execute_batch(
        "CREATE TRIGGER reject_latest_state_replacement
         BEFORE INSERT ON ceph_rocksdb_latest_state
         BEGIN
             SELECT RAISE(ABORT, 'injected latest-state write failure');
         END;",
    )
    .expect("install failure trigger");
    assert!(CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &replacement_1)
        .is_err());
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find("inventory-1")
            .expect("reload rolled back source"),
        expected_1
    );

    conn.execute_batch("DROP TRIGGER reject_latest_state_replacement")
        .expect("remove failure trigger");
    CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &replacement_1)
        .expect("commit replacement");
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find("inventory-2")
            .expect("source 2 remains isolated"),
        vec![original_2[1].clone(), original_2[0].clone()]
    );
}

#[test]
fn validation_requires_exactly_one_row_per_active_cf_and_no_dropped_cf() {
    let conn = setup();
    let rocksdb = seed_control_plane(&conn, "inventory-1", "source-1");
    let valid = records("inventory-1");
    validate_replacement(&rocksdb, &valid).expect("validate complete active set");

    assert!(validate_replacement(&rocksdb, &valid[..1]).is_err());

    let mut duplicate = valid.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(validate_replacement(&rocksdb, &duplicate).is_err());

    let mut dropped = valid.clone();
    dropped[1] = record("inventory-1", 2, "legacy", '2');
    assert!(validate_replacement(&rocksdb, &dropped).is_err());

    let mut wrong_name = valid;
    wrong_name[0].column_family_name = "wrong".to_string();
    assert!(validate_replacement(&rocksdb, &wrong_name).is_err());
}

#[test]
fn validation_rejects_invalid_hashes_counts_sequences_and_partial_scans() {
    let conn = setup();
    let rocksdb = seed_control_plane(&conn, "inventory-1", "source-1");
    let valid = records("inventory-1");

    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.point_sha256 = "A".repeat(64);
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.inventory_id.clear();
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.wal_point_mutation_count += 1;
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.deleted_key_count += 1;
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.merge_resolved_count = record.merge_operand_count + 1;
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.largest_sequence = Some(MAX_SEQUENCE + 1);
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.smallest_sequence = None;
    });
    assert_invalid_mutation(&rocksdb, &valid, |record| {
        record.scan_complete = false;
    });

    let mut mismatched_sharding = valid;
    mismatched_sharding[0].sharding_sha256 = "f".repeat(64);
    assert!(validate_replacement(&rocksdb, &mismatched_sharding).is_err());
}

fn assert_invalid_mutation(
    rocksdb: &CephRocksdbAggregate,
    valid: &[CephRocksdbLatestStateRecord],
    mutate: impl FnOnce(&mut CephRocksdbLatestStateRecord),
) {
    let mut invalid = valid.to_vec();
    mutate(&mut invalid[0]);
    assert!(validate_replacement(rocksdb, &invalid).is_err());
}

#[test]
fn replacement_rejects_cross_inventory_rows_and_cascades_with_control_plane() {
    let conn = setup();
    let rocksdb = seed_control_plane(&conn, "inventory-1", "source-1");
    let valid = records("inventory-1");
    validate_replacement(&rocksdb, &valid).expect("validate latest state");
    CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &valid)
        .expect("insert latest state");

    assert!(CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-2", &valid)
        .is_err());
    assert!(CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &[])
        .is_err());
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
         ) VALUES (
            ?1, 1, 'scox-v1', ?2, ?3, ?4,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1
         )",
        rusqlite::params![
            "inventory-1",
            valid[0].sharding_sha256,
            latest_state_set_sha256(&valid),
            "f".repeat(64),
        ],
    )
    .expect("insert dependent semantic marker");
    assert!(CephRocksdbLatestStateRepo::new(&conn)
        .replace_for_inventory("inventory-1", &valid)
        .is_err());
    assert_eq!(
        CephRocksdbLatestStateRepo::new(&conn)
            .find("inventory-1")
            .expect("latest state remains intact"),
        vec![valid[1].clone(), valid[0].clone()]
    );
    conn.execute(
        "DELETE FROM ceph_rocksdb_manifests WHERE inventory_id = ?1",
        ["inventory-1"],
    )
    .expect("delete control plane");
    assert!(CephRocksdbLatestStateRepo::new(&conn)
        .find("inventory-1")
        .expect("find cascaded latest state")
        .is_empty());
}

#[test]
fn latest_state_set_digest_is_independent_of_caller_order() {
    let ordered = records("inventory-1");
    let reversed = ordered.iter().rev().cloned().collect::<Vec<_>>();
    assert_eq!(
        latest_state_set_sha256(&ordered),
        latest_state_set_sha256(&reversed)
    );
}
