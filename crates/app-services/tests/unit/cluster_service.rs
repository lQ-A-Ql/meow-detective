use super::*;
use domain::DataSourcePlatform;

#[test]
fn topology_scope_and_edge_kinds_have_stable_storage_keys() {
    assert_eq!(TopologyScopeKind::PhysicalHost.as_str(), "physical_host");
    assert_eq!(TopologyScopeKind::Pve.as_str(), "pve");
    assert_eq!(TopologyScopeKind::Ceph.as_str(), "ceph");
    assert_eq!(
        TopologyScopeKind::VirtualMachine.as_str(),
        "virtual_machine"
    );
    assert_eq!(TopologyScopeKind::OsInstance.as_str(), "os_instance");
    assert_eq!(TopologyScopeKind::Kubernetes.as_str(), "kubernetes");
    assert_eq!(
        TopologyEdgeKind::ProvidesStorage.as_str(),
        "provides_storage"
    );
    assert_eq!(TopologyEdgeKind::Runs.as_str(), "runs");
}

#[test]
fn evidence_set_plan_discovers_supported_images_without_declaring_a_platform() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("node-b.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("node-a.E01"), b"short").unwrap();
    std::fs::write(tmp.path().join("node-a.E02"), b"segment").unwrap();
    std::fs::write(tmp.path().join("notes.txt"), b"ignore").unwrap();

    let plan = plan_linux_evidence_set_import(tmp.path(), Some("host-set".to_string())).unwrap();

    assert_eq!(plan.import_set_name, "host-set");
    assert_eq!(plan.members.len(), 2);
    assert_eq!(plan.members[0].source_name, "node-a.E01");
    assert_eq!(plan.members[1].source_name, "node-b.raw");
    assert!(plan.manifest_rel_path.starts_with("import-sets/"));
}

#[test]
fn evidence_set_plan_preserves_nested_member_order() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server01 = tmp.path().join("server01");
    let server02 = tmp.path().join("server02");
    std::fs::create_dir_all(&server01).unwrap();
    std::fs::create_dir_all(&server02).unwrap();
    std::fs::write(server01.join("server01-disk01.E01"), b"e01").unwrap();
    std::fs::write(server01.join("server01-disk02.E01"), b"e01").unwrap();
    std::fs::write(server02.join("server02-disk01.raw"), b"raw").unwrap();

    let plan = plan_linux_evidence_set_import(tmp.path(), None).unwrap();
    let paths = plan
        .members
        .iter()
        .map(|member| {
            member
                .source_path
                .strip_prefix(tmp.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        vec![
            "server01/server01-disk01.E01",
            "server01/server01-disk02.E01",
            "server02/server02-disk01.raw",
        ]
    );
    assert_eq!(plan.members[2].member_index, 2);
}

#[test]
fn evidence_set_requires_multiple_images() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("single.raw"), b"raw").unwrap();

    assert!(matches!(
        plan_linux_evidence_set_import(tmp.path(), None),
        Err(ClusterServiceError::InsufficientSources)
    ));
}

#[test]
fn evidence_set_member_configs_are_linux_scoped_not_cluster_profiled() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
    let plan = plan_linux_evidence_set_import(tmp.path(), None).unwrap();
    let configs = plan.member_import_configs();

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].platform, DataSourcePlatform::Linux);
    assert_eq!(configs[0].profile, None);
    assert_eq!(
        configs[0]
            .import_set
            .as_ref()
            .map(|value| value.import_set_id.as_str()),
        Some(plan.import_set_id.as_str())
    );
}

#[test]
fn evidence_set_manifest_write_is_atomic_and_readable() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("a.E02"), b"segment").unwrap();
    let plan = plan_linux_evidence_set_import(tmp.path(), None).unwrap();
    let case_root = tempfile::TempDir::new().unwrap();

    let manifest_path = write_linux_evidence_set_manifest(case_root.path(), &plan).unwrap();
    let manifest = std::fs::read_to_string(manifest_path).unwrap();

    assert!(manifest.contains(&plan.import_set_id));
    assert!(manifest.contains("\"memberCount\": 2"));
    assert!(manifest.contains("\"manifestDigest\": "));
    assert!(manifest.contains("\"sourceSizeBytes\": 3"));
    assert!(manifest.contains("\"hashStatus\": \"pending\""));
    assert!(manifest.contains("\"scanPolicy\": \"recursive-root-relative-v1\""));
    assert!(manifest.contains("\"secondary_e01_segment\""));
}

#[test]
fn evidence_set_manifest_hash_update_rewrites_digest_and_member_status() {
    let source_root = tempfile::TempDir::new().unwrap();
    std::fs::write(source_root.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(source_root.path().join("b.raw"), b"raw").unwrap();
    let plan = plan_linux_evidence_set_import(source_root.path(), None).unwrap();
    let case_root = tempfile::TempDir::new().unwrap();
    write_linux_evidence_set_manifest(case_root.path(), &plan).unwrap();

    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-manifest', 'manifest')",
        [],
    )
    .unwrap();
    let member = &plan.members[0];
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-manifest', 'case-manifest', 'a', 'raw', ?1, datetime('now'))",
        [member.source_path.display().to_string()],
    )
    .unwrap();
    let repo =
        persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo::new(&conn);
    repo.insert(
        &persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRecord {
            id: plan.import_set_id.clone(),
            case_id: "case-manifest".to_string(),
            name: plan.import_set_name.clone(),
            root_path: plan.root_path.display().to_string(),
            import_state: "ready".to_string(),
            member_count: 2,
            ready_count: 2,
            failed_count: 0,
            last_error: None,
        },
    )
    .unwrap();
    repo.insert_member(
        &persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetMemberRecord {
            import_set_id: plan.import_set_id.clone(),
            member_index: member.member_index,
            source_path: member.source_path.display().to_string(),
            source_kind: "raw".to_string(),
            data_source_id: Some("source-manifest".to_string()),
            import_state: "ready".to_string(),
            last_error: None,
        },
    )
    .unwrap();

    update_linux_evidence_set_manifest_hash(
        case_root.path(),
        &conn,
        "source-manifest",
        "hashed",
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
    )
    .unwrap();
    let manifest_path = case_root.path().join(&plan.manifest_rel_path);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["members"][0]["hashStatus"], "hashed");
    assert_eq!(
        manifest["members"][0]["sourceSha256"],
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    assert!(manifest["manifestDigest"].as_str().is_some());
}

#[test]
fn ceph_scope_report_round_trips_and_rejects_path_components() {
    let case_root = tempfile::TempDir::new().unwrap();
    let report = crate::ceph_reconstruction::InventoryCoverageReport {
        policy: crate::ceph_reconstruction::RbdReplicaPolicy::strict_legacy(),
        expected_count: 3,
        observed_count: 3,
        state: crate::ceph_reconstruction::InventoryCoverageState::Complete,
        duplicate_inventory_ids: Vec::new(),
        duplicate_source_ids: Vec::new(),
        duplicate_osd_ids: Vec::new(),
        ceph_fsids: vec!["fsid".to_string()],
        diagnostics: Vec::new(),
    };
    write_ceph_scope_coverage_report(case_root.path(), "scope:ceph:1", &report).unwrap();
    assert_eq!(
        read_ceph_scope_coverage_report(case_root.path(), "scope:ceph:1").unwrap(),
        Some(report.clone())
    );
    assert!(matches!(
        write_ceph_scope_coverage_report(case_root.path(), "../escape", &report),
        Err(ClusterServiceError::InvalidClusterId)
    ));
}

#[path = "cluster_service/kubernetes.rs"]
mod kubernetes;

#[path = "cluster_service/topology_projection.rs"]
mod topology_projection;

#[path = "cluster_service/capability.rs"]
mod capability;

#[path = "cluster_service/kubernetes_parsers.rs"]
mod kubernetes_parsers;

#[path = "cluster_service/network_parser_resources.rs"]
mod network_parser_resources;
