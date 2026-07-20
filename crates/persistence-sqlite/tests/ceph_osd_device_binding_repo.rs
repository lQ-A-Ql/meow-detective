use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_osd_device_binding_repo::{
            CephOsdDeviceBindingAggregate, CephOsdDeviceBindingRecord, CephOsdDeviceBindingRepo,
            CephOsdPvBindingRecord,
        },
        ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
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
                id, case_id, name, kind, source_path, canonical_source_path, imported_at
             ) VALUES (?1, 'case-1', ?1, 'raw', ?2, ?2, '2026-07-15T00:00:00Z')",
            [data_source_id, &format!("C:/evidence/{data_source_id}.raw")],
        )
        .expect("insert source metadata");
    }
    conn
}

fn inventory(id: &str, data_source_id: &str) -> CephOsdInventoryRecord {
    CephOsdInventoryRecord {
        id: id.to_string(),
        data_source_id: data_source_id.to_string(),
        partition_index: None,
        lvm_vg_uuid: Some("vg-uuid".to_string()),
        lvm_vg_name: Some("ceph-vg".to_string()),
        lvm_lv_uuid: Some("lv-uuid".to_string()),
        lvm_lv_name: Some("osd-block".to_string()),
        osd_uuid: format!("osd-{id}"),
        ceph_fsid: None,
        whoami: None,
        device_role: "block".to_string(),
        device_size: 4096,
        birth_time_seconds: 0,
        birth_time_nanoseconds: 0,
        description: "device".to_string(),
        is_multi: false,
        selected_epoch: None,
        valid_label_count: 1,
        label_health: "singleReplica".to_string(),
        osd_key_present: false,
        kv_backend: None,
        bluefs_enabled: Some(false),
        ceph_version_when_created: None,
        require_osd_release: None,
        sanitized_metadata_json: "{}".to_string(),
    }
}

fn binding(inventory_id: &str, data_source_id: &str) -> CephOsdDeviceBindingAggregate {
    let source_path = format!("C:/evidence/{data_source_id}.raw");
    CephOsdDeviceBindingAggregate {
        device: CephOsdDeviceBindingRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            source_path: source_path.clone(),
            canonical_source_path: source_path.clone(),
            source_kind: "raw".to_string(),
            lvm_vg_uuid: "vg-uuid".to_string(),
            lvm_vg_name: "ceph-vg".to_string(),
            lvm_lv_uuid: "lv-uuid".to_string(),
            lvm_lv_name: "osd-block".to_string(),
            device_size: 4096,
        },
        physical_volumes: vec![CephOsdPvBindingRecord {
            inventory_id: inventory_id.to_string(),
            ordinal: 0,
            source_path: source_path.clone(),
            canonical_source_path: source_path,
            source_kind: "raw".to_string(),
            pv_offset: 2048,
            pv_uuid: format!("pv-{data_source_id}"),
            pv_name: Some("pv0".to_string()),
        }],
    }
}

#[test]
fn source_migration_installs_device_binding_schema() {
    let conn = setup_source_db();

    assert_eq!(
        runner::latest_source_version(),
        "source_021_cephfs_assembly_capability"
    );
    for table in ["ceph_osd_device_bindings", "ceph_osd_device_binding_pvs"] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query binding table");
        assert!(exists, "missing table {table}");
    }
}

#[test]
fn binding_round_trips_and_is_source_isolated() {
    let conn = setup_source_db();
    let inventory = inventory("inventory-1", "source-1");
    let binding = binding("inventory-1", "source-1");
    CephOsdRepo::new(&conn)
        .replace_for_data_source_with_device_bindings(
            "source-1",
            std::slice::from_ref(&inventory),
            &[],
            std::slice::from_ref(&binding),
        )
        .expect("persist source-bound device");

    let repo = CephOsdDeviceBindingRepo::new(&conn);
    let loaded = repo
        .find_source_bound_device("source-1", "inventory-1")
        .expect("query source-bound device")
        .expect("binding exists");
    assert_eq!(loaded.binding, binding);
    assert_eq!(loaded.source.data_source_id, "source-1");
    assert!(repo
        .find_source_bound_device("source-2", "inventory-1")
        .expect("query other source")
        .is_none());
}

#[test]
fn replacement_cascades_old_binding_without_touching_other_source() {
    let conn = setup_source_db();
    for data_source_id in ["source-1", "source-2"] {
        let inventory_id = format!("inventory-{data_source_id}");
        CephOsdRepo::new(&conn)
            .replace_for_data_source_with_device_bindings(
                data_source_id,
                &[inventory(&inventory_id, data_source_id)],
                &[],
                &[binding(&inventory_id, data_source_id)],
            )
            .expect("persist source binding");
    }

    let replacement = inventory("inventory-new", "source-1");
    CephOsdRepo::new(&conn)
        .replace_for_data_source_with_device_bindings(
            "source-1",
            std::slice::from_ref(&replacement),
            &[],
            &[binding("inventory-new", "source-1")],
        )
        .expect("replace source binding");

    let repo = CephOsdDeviceBindingRepo::new(&conn);
    assert!(repo
        .find_source_bound_device("source-1", "inventory-source-1")
        .expect("query removed binding")
        .is_none());
    assert!(repo
        .find_source_bound_device("source-2", "inventory-source-2")
        .expect("query isolated binding")
        .is_some());
    let orphan_count: u64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM ceph_osd_device_binding_pvs pv
             LEFT JOIN ceph_osd_device_bindings binding
               ON binding.inventory_id = pv.inventory_id
             WHERE binding.inventory_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("count orphan PV bindings");
    assert_eq!(orphan_count, 0);
}

#[test]
fn invalid_binding_does_not_delete_existing_aggregate() {
    let conn = setup_source_db();
    let existing_inventory = inventory("inventory-existing", "source-1");
    let existing_binding = binding("inventory-existing", "source-1");
    CephOsdRepo::new(&conn)
        .replace_for_data_source_with_device_bindings(
            "source-1",
            std::slice::from_ref(&existing_inventory),
            &[],
            std::slice::from_ref(&existing_binding),
        )
        .expect("persist existing binding");

    let replacement = inventory("inventory-new", "source-1");
    let mut invalid = binding("inventory-new", "source-1");
    invalid.physical_volumes[0].ordinal = 1;
    assert!(CephOsdRepo::new(&conn)
        .replace_for_data_source_with_device_bindings(
            "source-1",
            std::slice::from_ref(&replacement),
            &[],
            std::slice::from_ref(&invalid),
        )
        .is_err());

    assert_eq!(
        CephOsdDeviceBindingRepo::new(&conn)
            .find_source_bound_device("source-1", "inventory-existing")
            .expect("query preserved binding")
            .expect("existing binding remains")
            .binding,
        existing_binding
    );
}
