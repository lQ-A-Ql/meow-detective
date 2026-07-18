use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluefs_replay_repo::{
            CephBluefsDirectoryRecord, CephBluefsFileExtentRecord, CephBluefsFileRecord,
            CephBluefsReplayAggregate, CephBluefsReplayRecord, CephBluefsReplayRepo,
        },
        ceph_bluefs_repo::{
            CephBluefsAggregate, CephBluefsLogExtentRecord, CephBluefsRepo,
            CephBluefsSuperblockRecord,
        },
        ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
    },
    runner,
};

fn setup_source_db() -> rusqlite::Connection {
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
    insert_osd_inventory(&conn, "inventory-1", "source-1", "osd-1");
    insert_osd_inventory(&conn, "inventory-2", "source-2", "osd-2");
    conn
}

fn insert_osd_inventory(
    conn: &rusqlite::Connection,
    id: &str,
    data_source_id: &str,
    osd_uuid: &str,
) {
    let record = CephOsdInventoryRecord {
        id: id.to_string(),
        data_source_id: data_source_id.to_string(),
        partition_index: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        osd_uuid: osd_uuid.to_string(),
        ceph_fsid: None,
        whoami: None,
        device_role: "bluestore".to_string(),
        device_size: 1024 * 1024,
        birth_time_seconds: 0,
        birth_time_nanoseconds: 0,
        description: "main".to_string(),
        is_multi: false,
        selected_epoch: None,
        valid_label_count: 1,
        label_health: "singleReplica".to_string(),
        osd_key_present: false,
        kv_backend: Some("rocksdb".to_string()),
        bluefs_enabled: Some(true),
        ceph_version_when_created: None,
        require_osd_release: None,
        sanitized_metadata_json: "{}".to_string(),
    };
    CephOsdRepo::new(conn)
        .replace_for_data_source(data_source_id, &[record], &[])
        .expect("insert OSD inventory");
}

fn superblock(
    inventory_id: &str,
    data_source_id: &str,
    sequence: u64,
) -> CephBluefsSuperblockRecord {
    let osd_suffix = inventory_id
        .strip_prefix("inventory-")
        .unwrap_or(inventory_id);
    CephBluefsSuperblockRecord {
        inventory_id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        bluefs_uuid: format!("bluefs-{inventory_id}"),
        osd_uuid: format!("osd-{osd_suffix}"),
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
    }
}

fn extent(inventory_id: &str, ordinal: u32, offset: u64) -> CephBluefsLogExtentRecord {
    CephBluefsLogExtentRecord {
        inventory_id: inventory_id.to_string(),
        ordinal,
        device_id: 1,
        offset,
        length: 64 * 1024,
    }
}

fn replay(inventory_id: &str, final_sequence: u64) -> CephBluefsReplayAggregate {
    CephBluefsReplayAggregate {
        replay: CephBluefsReplayRecord {
            inventory_id: inventory_id.to_string(),
            transaction_count: 4,
            first_sequence: 1,
            final_sequence,
            logical_bytes: 0x22_000,
            stop_reason: "invalidTail".to_string(),
        },
        directories: vec![CephBluefsDirectoryRecord {
            inventory_id: inventory_id.to_string(),
            path: "db".to_string(),
        }],
        files: vec![CephBluefsFileRecord {
            inventory_id: inventory_id.to_string(),
            path: "db/CURRENT".to_string(),
            inode: 2,
            size: 16,
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 123,
            encoding: 0,
            content_size: 16,
        }],
        file_extents: vec![CephBluefsFileExtentRecord {
            inventory_id: inventory_id.to_string(),
            file_path: "db/CURRENT".to_string(),
            ordinal: 0,
            device_id: 1,
            offset: 512 * 1024,
            length: 4096,
        }],
    }
}

fn aggregate(
    superblock: &CephBluefsSuperblockRecord,
    extents: &[CephBluefsLogExtentRecord],
) -> CephBluefsAggregate {
    CephBluefsAggregate {
        superblock: superblock.clone(),
        log_extents: extents.to_vec(),
        replay: replay(&superblock.inventory_id, superblock.sequence),
    }
}

fn persist_bluefs(
    conn: &rusqlite::Connection,
    superblock: &CephBluefsSuperblockRecord,
    extents: &[CephBluefsLogExtentRecord],
) -> persistence_sqlite::connection::DbResult<()> {
    let osd_repo = CephOsdRepo::new(conn);
    let inventory = osd_repo.find_by_data_source(&superblock.data_source_id)?;
    let records = aggregate(superblock, extents);
    osd_repo.replace_for_data_source_with_bluefs(
        &superblock.data_source_id,
        &inventory,
        &[],
        Some(&records),
    )
}

#[test]
fn source_migration_installs_bluefs_inventory_schema() {
    let conn = setup_source_db();

    assert_eq!(
        runner::latest_source_version(),
        "source_017_timeline_projection_identity"
    );
    for table in [
        "ceph_bluefs_superblocks",
        "ceph_bluefs_log_extents",
        "ceph_bluefs_replays",
        "ceph_bluefs_directories",
        "ceph_bluefs_files",
        "ceph_bluefs_file_extents",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query table");
        assert!(exists, "missing table {table}");
    }
}

#[test]
fn superblock_and_log_extents_round_trip_in_stable_order() {
    let conn = setup_source_db();
    let repo = CephBluefsRepo::new(&conn);
    let expected = superblock("inventory-1", "source-1", 50);
    let expected_extents = vec![
        extent("inventory-1", 0, 172 * 1024 * 1024),
        extent("inventory-1", 1, 256 * 1024 * 1024),
    ];

    persist_bluefs(&conn, &expected, &expected_extents).expect("persist BlueFS inventory");

    assert_eq!(
        repo.find_by_data_source("source-1")
            .expect("load superblock"),
        vec![expected]
    );
    assert_eq!(
        repo.find_log_extents("inventory-1").expect("load extents"),
        expected_extents
    );
    let replay_repo = CephBluefsReplayRepo::new(&conn);
    let expected_replay = replay("inventory-1", 50);
    assert_eq!(
        replay_repo.find_replay("inventory-1").unwrap(),
        Some(expected_replay.replay)
    );
    assert_eq!(
        replay_repo.find_directories("inventory-1").unwrap(),
        expected_replay.directories
    );
    assert_eq!(
        replay_repo.find_files("inventory-1").unwrap(),
        expected_replay.files
    );
    assert_eq!(
        replay_repo
            .find_file_extents("inventory-1", "db/CURRENT")
            .unwrap(),
        expected_replay.file_extents
    );
}

#[test]
fn replacement_removes_old_extents_without_touching_other_sources() {
    let conn = setup_source_db();
    let repo = CephBluefsRepo::new(&conn);
    persist_bluefs(
        &conn,
        &superblock("inventory-1", "source-1", 1),
        &[extent("inventory-1", 0, 4096)],
    )
    .expect("persist source 1");
    persist_bluefs(
        &conn,
        &superblock("inventory-2", "source-2", 2),
        &[extent("inventory-2", 0, 8192)],
    )
    .expect("persist source 2");

    persist_bluefs(
        &conn,
        &superblock("inventory-1", "source-1", 3),
        &[extent("inventory-1", 0, 12_288)],
    )
    .expect("replace source 1");

    assert_eq!(
        repo.find_log_extents("inventory-1").unwrap(),
        vec![extent("inventory-1", 0, 12_288)]
    );
    assert_eq!(
        repo.find_log_extents("inventory-2").unwrap(),
        vec![extent("inventory-2", 0, 8192)]
    );
}

#[test]
fn invalid_extent_reference_does_not_delete_existing_inventory() {
    let conn = setup_source_db();
    let repo = CephBluefsRepo::new(&conn);
    let existing = superblock("inventory-1", "source-1", 1);
    persist_bluefs(&conn, &existing, &[extent("inventory-1", 0, 4096)])
        .expect("persist existing inventory");

    let result = persist_bluefs(
        &conn,
        &superblock("inventory-1", "source-1", 2),
        &[extent("inventory-2", 0, 8192)],
    );

    assert!(result.is_err());
    assert_eq!(
        repo.find_by_data_source("source-1").unwrap(),
        vec![existing]
    );
}

#[test]
fn cross_source_or_osd_binding_is_rejected_atomically() {
    let conn = setup_source_db();
    let repo = CephBluefsRepo::new(&conn);
    let existing = superblock("inventory-1", "source-1", 1);
    persist_bluefs(&conn, &existing, &[extent("inventory-1", 0, 4096)])
        .expect("persist existing inventory");

    let mut cross_source = superblock("inventory-1", "source-2", 2);
    cross_source.osd_uuid = "osd-2".to_string();
    assert!(persist_bluefs(&conn, &cross_source, &[extent("inventory-1", 0, 8192)]).is_err());

    assert_eq!(
        repo.find_by_data_source("source-1").unwrap(),
        vec![existing]
    );
    assert!(repo.find_by_data_source("source-2").unwrap().is_empty());
}

#[test]
fn osd_and_bluefs_inventory_commit_atomically() {
    let conn = setup_source_db();
    let osd_repo = CephOsdRepo::new(&conn);
    let existing = osd_repo
        .find_by_data_source("source-1")
        .expect("load existing OSD inventory")
        .pop()
        .expect("existing OSD inventory");
    let mut replacement = existing.clone();
    replacement.description = "replacement".to_string();
    let existing_bluefs = superblock("inventory-1", "source-1", 1);
    let existing_extents = vec![extent("inventory-1", 0, 8192)];
    let existing_records = aggregate(&existing_bluefs, &existing_extents);
    osd_repo
        .replace_for_data_source_with_bluefs(
            "source-1",
            std::slice::from_ref(&existing),
            &[],
            Some(&existing_records),
        )
        .expect("persist existing OSD and BlueFS inventory");
    let mut invalid_bluefs = superblock("inventory-1", "source-1", u64::MAX);
    invalid_bluefs.osd_uuid = replacement.osd_uuid.clone();
    let invalid_records = aggregate(&invalid_bluefs, &[extent("inventory-1", 0, 4096)]);

    let result = osd_repo.replace_for_data_source_with_bluefs(
        "source-1",
        &[replacement],
        &[],
        Some(&invalid_records),
    );

    assert!(result.is_err());
    assert_eq!(
        osd_repo
            .find_by_data_source("source-1")
            .expect("reload OSD inventory"),
        vec![existing]
    );
    let bluefs_repo = CephBluefsRepo::new(&conn);
    assert_eq!(
        bluefs_repo
            .find_by_data_source("source-1")
            .expect("load BlueFS inventory"),
        vec![existing_bluefs]
    );
    assert_eq!(
        bluefs_repo
            .find_log_extents("inventory-1")
            .expect("load BlueFS extents"),
        existing_extents
    );
    assert_eq!(
        CephBluefsReplayRepo::new(&conn)
            .find_replay("inventory-1")
            .expect("load BlueFS replay"),
        Some(existing_records.replay.replay)
    );
}
