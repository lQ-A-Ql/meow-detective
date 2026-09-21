use crate::{cluster_service::project_import_set_topology, source_db};
use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    case_repo::CaseRepo,
    ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
    linux_import_set_repo::{LinuxImportSetMemberRecord, LinuxImportSetRecord, LinuxImportSetRepo},
};

#[test]
fn projection_separates_pve_ceph_os_and_kubernetes_scopes() {
    let root = tempfile::TempDir::new().unwrap();
    let conn = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = CaseId("case-topology-projection".to_string());
    CaseRepo::new(&conn)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "topology".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    let import_set_id = "evidence-set";
    let set_repo = LinuxImportSetRepo::new(&conn);
    set_repo
        .insert(&LinuxImportSetRecord {
            id: import_set_id.to_string(),
            case_id: case_id.0.clone(),
            name: "evidence".to_string(),
            root_path: "evidence".to_string(),
            import_state: "ready".to_string(),
            member_count: 2,
            ready_count: 2,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
    register_source(
        &conn,
        root.path(),
        &case_id,
        import_set_id,
        0,
        "pve-node",
        &[
            "/etc/pve/qemu-server/100.conf",
            "/etc/kubernetes/manifests/kube-apiserver.yaml",
        ],
    );
    register_source(
        &conn,
        root.path(),
        &case_id,
        import_set_id,
        1,
        "ceph-node",
        &[],
    );
    let ceph_id = DataSourceId("ceph-node".to_string());
    let source_conn = source_db::open_source_db(root.path(), &ceph_id).unwrap();
    CephOsdRepo::new(&source_conn)
        .replace_for_data_source(
            &ceph_id.0,
            &[CephOsdInventoryRecord {
                id: "osd-0".to_string(),
                data_source_id: ceph_id.0.clone(),
                partition_index: Some(0),
                lvm_vg_uuid: None,
                lvm_vg_name: None,
                lvm_lv_uuid: None,
                lvm_lv_name: None,
                osd_uuid: "11111111-1111-1111-1111-111111111111".to_string(),
                ceph_fsid: Some("22222222-2222-2222-2222-222222222222".to_string()),
                whoami: Some(0),
                device_role: "block".to_string(),
                device_size: 1,
                birth_time_seconds: 1,
                birth_time_nanoseconds: 0,
                description: "osd".to_string(),
                is_multi: false,
                selected_epoch: Some(1),
                valid_label_count: 1,
                label_health: "valid".to_string(),
                osd_key_present: false,
                kv_backend: None,
                bluefs_enabled: None,
                ceph_version_when_created: None,
                require_osd_release: None,
                sanitized_metadata_json: "{}".to_string(),
            }],
            &[],
        )
        .unwrap();

    let projection =
        project_import_set_topology(&conn, root.path(), &case_id, import_set_id).unwrap();
    assert_eq!(projection.os_scope_ids.len(), 2);
    assert_eq!(
        projection.pve_scope_id.as_deref(),
        Some("scope:pve:evidence-set")
    );
    assert_eq!(
        projection.ceph_scope_id.as_deref(),
        Some("scope:ceph:evidence-set")
    );
    assert_eq!(
        projection.kubernetes_scope_id.as_deref(),
        Some("scope:kubernetes:evidence-set")
    );
    assert_eq!(
        projection.virtual_machine_scope_ids,
        vec!["scope:vm:evidence-set:100"]
    );

    let edges = conn.prepare("SELECT source_scope_id, target_scope_id, edge_kind FROM linux_topology_edges ORDER BY source_scope_id, target_scope_id, edge_kind").unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))).unwrap()
        .collect::<Result<Vec<_>, _>>().unwrap();
    assert!(edges.contains(&(
        "scope:pve:evidence-set".to_string(),
        "scope:vm:evidence-set:100".to_string(),
        "hosts".to_string(),
    )));
    assert!(edges.contains(&(
        "scope:ceph:evidence-set".to_string(),
        "scope:pve:evidence-set".to_string(),
        "provides_storage".to_string(),
    )));
    let k8_artifacts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM linux_topology_artifacts WHERE scope_id = 'scope:kubernetes:evidence-set'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(k8_artifacts, 1);
    let pve_source_db =
        source_db::open_source_db(root.path(), &DataSourceId("pve-node".to_string())).unwrap();
    assert_eq!(
        ArtifactRepo::new(&pve_source_db).count().unwrap(),
        0,
        "Kubernetes discovery belongs to topology artifacts, not Linux artifact families",
    );
}

#[test]
fn projection_refuses_an_incomplete_evidence_set_before_creating_scopes() {
    let root = tempfile::TempDir::new().unwrap();
    let conn = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = CaseId("case-incomplete-evidence-set".to_string());
    CaseRepo::new(&conn)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "incomplete".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    let repo = LinuxImportSetRepo::new(&conn);
    repo.insert(&LinuxImportSetRecord {
        id: "incomplete-set".to_string(),
        case_id: case_id.0.clone(),
        name: "set".to_string(),
        root_path: "evidence".to_string(),
        import_state: "failed".to_string(),
        member_count: 1,
        ready_count: 0,
        failed_count: 1,
        last_error: Some("failed".to_string()),
    })
    .unwrap();
    repo.insert_member(&LinuxImportSetMemberRecord {
        import_set_id: "incomplete-set".to_string(),
        member_index: 0,
        source_path: "failed.raw".to_string(),
        source_kind: "raw".to_string(),
        data_source_id: None,
        import_state: "failed".to_string(),
        last_error: Some("failed".to_string()),
    })
    .unwrap();

    assert!(matches!(
        project_import_set_topology(&conn, root.path(), &case_id, "incomplete-set"),
        Err(crate::cluster_service::ClusterServiceError::IncompleteImportSet)
    ));
    let scope_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM linux_topology_scopes", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(scope_count, 0);
}

#[test]
fn projection_does_not_create_kubernetes_scope_without_kubernetes_evidence() {
    let root = tempfile::TempDir::new().unwrap();
    let conn = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = CaseId("case-no-kubernetes".to_string());
    CaseRepo::new(&conn)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "no kubernetes".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    let set_repo = LinuxImportSetRepo::new(&conn);
    set_repo
        .insert(&LinuxImportSetRecord {
            id: "plain-linux-set".to_string(),
            case_id: case_id.0.clone(),
            name: "plain".to_string(),
            root_path: "evidence".to_string(),
            import_state: "ready".to_string(),
            member_count: 2,
            ready_count: 2,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
    register_source(
        &conn,
        root.path(),
        &case_id,
        "plain-linux-set",
        0,
        "plain-a",
        &["/etc/os-release"],
    );
    register_source(
        &conn,
        root.path(),
        &case_id,
        "plain-linux-set",
        1,
        "plain-b",
        &["/var/log/messages"],
    );

    let projection =
        project_import_set_topology(&conn, root.path(), &case_id, "plain-linux-set").unwrap();
    assert_eq!(projection.kubernetes_scope_id, None);
    let k8_scope_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM linux_topology_scopes WHERE scope_kind = 'kubernetes'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(k8_scope_count, 0);
}

fn register_source(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    member_index: u32,
    source_id: &str,
    paths: &[&str],
) {
    let id = DataSourceId(source_id.to_string());
    let source = DataSource {
        id: id.clone(),
        name: source_id.to_string(),
        kind: DataSourceKind::Raw,
        source_path: std::path::PathBuf::from(format!("{source_id}.raw")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(conn)
        .insert_with_storage(case_id, &source, &storage)
        .unwrap();
    LinuxImportSetRepo::new(conn)
        .insert_member(&LinuxImportSetMemberRecord {
            import_set_id: import_set_id.to_string(),
            member_index,
            source_path: source.source_path.display().to_string(),
            source_kind: "raw".to_string(),
            data_source_id: Some(id.0.clone()),
            import_state: "ready".to_string(),
            last_error: None,
        })
        .unwrap();
    let source_conn = source_db::open_source_db(root, &id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(case_id, &source)
        .unwrap();
    let entries = paths
        .iter()
        .enumerate()
        .map(|(index, path)| FileEntry {
            id: FileEntryId(format!("{source_id}-file-{index}")),
            parent_id: None,
            data_source_id: id.clone(),
            path: (*path).to_string(),
            name: path.rsplit('/').next().unwrap().to_string(),
            entry_type: EntryType::File,
            size: Some(1),
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: true,
            archive: false,
            unix_mode: Some(0o644),
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        })
        .collect::<Vec<_>>();
    FileRepo::new(&source_conn).insert_batch(&entries).unwrap();
}
