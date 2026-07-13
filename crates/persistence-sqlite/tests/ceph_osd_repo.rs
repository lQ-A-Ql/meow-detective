use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_osd_repo::{CephOsdInventoryRecord, CephOsdLabelReplicaRecord, CephOsdRepo},
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

fn inventory(id: &str, data_source_id: &str, whoami: u32) -> CephOsdInventoryRecord {
    CephOsdInventoryRecord {
        id: id.to_string(),
        data_source_id: data_source_id.to_string(),
        partition_index: Some(2),
        lvm_vg_uuid: Some("vg-uuid".to_string()),
        lvm_vg_name: Some("ceph-vg".to_string()),
        lvm_lv_uuid: Some(format!("lv-{whoami}")),
        lvm_lv_name: Some(format!("osd-block-{whoami}")),
        osd_uuid: format!("00000000-0000-0000-0000-{whoami:012}"),
        ceph_fsid: Some("11111111-2222-3333-4444-555555555555".to_string()),
        whoami: Some(whoami),
        device_role: "block".to_string(),
        device_size: 8 * 1024 * 1024 * 1024,
        birth_time_seconds: 1_700_000_000,
        birth_time_nanoseconds: 123_456_789,
        description: "主块设备".to_string(),
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

fn replica(inventory_id: &str, position: u64, selected: bool) -> CephOsdLabelReplicaRecord {
    CephOsdLabelReplicaRecord {
        inventory_id: inventory_id.to_string(),
        position,
        device_size: 8 * 1024 * 1024 * 1024,
        birth_time_seconds: 1_700_000_000,
        birth_time_nanoseconds: 123_456_789,
        description: "主块设备副本".to_string(),
        is_multi: true,
        epoch: Some(42),
        is_selected: selected,
        struct_version: 2,
        struct_compat_version: 1,
    }
}

#[test]
fn source_migration_installs_sanitized_inventory_schema() {
    let conn = setup_source_db();

    assert_eq!(
        runner::latest_source_version(),
        "source_005_ceph_osd_inventory"
    );
    for table in ["ceph_osd_inventory", "ceph_osd_label_replicas"] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query table");
        assert!(exists, "missing table {table}");

        let schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query table schema");
        assert!(!schema.to_ascii_lowercase().contains("osd_key text"));
    }
}

#[test]
fn inventory_and_label_replicas_round_trip_utf8() {
    let conn = setup_source_db();
    let repo = CephOsdRepo::new(&conn);
    let expected_inventory = inventory("inventory-1", "source-1", 7);
    let expected_replicas = vec![
        replica("inventory-1", 0, false),
        replica("inventory-1", 1 << 30, true),
    ];

    repo.replace_for_data_source(
        "source-1",
        std::slice::from_ref(&expected_inventory),
        &expected_replicas,
    )
    .expect("persist Ceph inventory");

    assert_eq!(
        repo.find_by_data_source("source-1")
            .expect("load inventory"),
        vec![expected_inventory]
    );
    assert_eq!(
        repo.find_label_replicas("inventory-1")
            .expect("load replicas"),
        expected_replicas
    );
}

#[test]
fn replacement_is_atomic_and_source_local() {
    let conn = setup_source_db();
    let repo = CephOsdRepo::new(&conn);
    let first = inventory("inventory-old", "source-1", 1);
    let other_source = inventory("inventory-other", "source-2", 2);
    repo.replace_for_data_source(
        "source-1",
        std::slice::from_ref(&first),
        &[replica("inventory-old", 0, true)],
    )
    .expect("persist first source");
    repo.replace_for_data_source(
        "source-2",
        std::slice::from_ref(&other_source),
        &[replica("inventory-other", 0, true)],
    )
    .expect("persist second source");

    let replacement = inventory("inventory-new", "source-1", 3);
    repo.replace_for_data_source(
        "source-1",
        std::slice::from_ref(&replacement),
        &[replica("inventory-new", 10 << 30, true)],
    )
    .expect("replace first source");

    assert_eq!(
        repo.find_by_data_source("source-1").expect("load source 1"),
        vec![replacement]
    );
    assert!(repo
        .find_label_replicas("inventory-old")
        .expect("load removed replicas")
        .is_empty());
    assert_eq!(
        repo.find_by_data_source("source-2").expect("load source 2"),
        vec![other_source]
    );
}

#[test]
fn invalid_replica_reference_does_not_delete_existing_inventory() {
    let conn = setup_source_db();
    let repo = CephOsdRepo::new(&conn);
    let existing = inventory("inventory-existing", "source-1", 1);
    repo.replace_for_data_source(
        "source-1",
        std::slice::from_ref(&existing),
        &[replica("inventory-existing", 0, true)],
    )
    .expect("persist existing inventory");

    let replacement = inventory("inventory-new", "source-1", 2);
    let result = repo.replace_for_data_source(
        "source-1",
        &[replacement],
        &[replica("unknown-inventory", 0, true)],
    );

    assert!(result.is_err());
    assert_eq!(
        repo.find_by_data_source("source-1")
            .expect("load preserved inventory"),
        vec![existing]
    );
}
