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
    let plan = plan_linux_evidence_set_import(tmp.path(), None).unwrap();
    let case_root = tempfile::TempDir::new().unwrap();

    let manifest_path = write_linux_evidence_set_manifest(case_root.path(), &plan).unwrap();
    let manifest = std::fs::read_to_string(manifest_path).unwrap();

    assert!(manifest.contains(&plan.import_set_id));
    assert!(manifest.contains("\"memberCount\": 2"));
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
