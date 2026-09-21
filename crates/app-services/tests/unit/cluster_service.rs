use super::*;
use domain::{DataSourceKind, DataSourcePlatform};
use std::path::PathBuf;

#[test]
fn linux_cluster_kind_gates_platform_specific_capabilities() {
    assert_eq!(
        LinuxClusterKind::from_profile(Some("kubernetes")),
        LinuxClusterKind::Kubernetes
    );
    assert!(LinuxClusterKind::Kubernetes.require_pve_ceph().is_err());
    assert!(LinuxClusterKind::PveCeph.require_pve_ceph().is_ok());
    assert!(LinuxClusterKind::Unknown.require_kubernetes().is_err());
}

#[path = "cluster_service/kubernetes.rs"]
mod kubernetes;

#[path = "cluster_service/kubernetes_parsers.rs"]
mod kubernetes_parsers;

#[test]
fn cluster_parse_plan_is_explicitly_non_executing() {
    let plan = plan_cluster_parse(ClusterParseRequest {
        sources: vec![
            ClusterEvidenceSource {
                source_path: PathBuf::from("node-a.E01"),
                source_kind: DataSourceKind::E01,
            },
            ClusterEvidenceSource {
                source_path: PathBuf::from("node-b.E01"),
                source_kind: DataSourceKind::E01,
            },
        ],
    })
    .unwrap();

    assert_eq!(plan.source_count, 2);
    assert!(!plan.supported_now);
    assert_eq!(
        plan.boundary,
        ClusterParseBoundary::PlannedPveLvmThinCluster
    );
}

#[test]
fn cluster_parse_execution_is_unsupported_in_single_disk_milestone() {
    let err = parse_cluster(ClusterParseRequest {
        sources: vec![
            ClusterEvidenceSource {
                source_path: PathBuf::from("node-a.E01"),
                source_kind: DataSourceKind::E01,
            },
            ClusterEvidenceSource {
                source_path: PathBuf::from("node-b.E01"),
                source_kind: DataSourceKind::E01,
            },
        ],
    })
    .unwrap_err();

    assert!(matches!(err, ClusterServiceError::Unsupported));
}

#[test]
fn cluster_parse_plan_requires_multiple_sources() {
    let err = plan_cluster_parse(ClusterParseRequest {
        sources: vec![ClusterEvidenceSource {
            source_path: PathBuf::from("single.E01"),
            source_kind: DataSourceKind::E01,
        }],
    })
    .unwrap_err();

    assert!(matches!(err, ClusterServiceError::InsufficientSources));
}

#[test]
fn linux_cluster_import_plan_discovers_supported_images() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("node-b.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("node-a.E01"), b"short").unwrap();
    std::fs::write(tmp.path().join("node-a.E02"), b"segment").unwrap();
    std::fs::write(tmp.path().join("notes.txt"), b"ignore").unwrap();

    let plan = plan_linux_cluster_import(tmp.path(), Some("pve-cluster".to_string())).unwrap();

    assert_eq!(plan.cluster_name, "pve-cluster");
    assert_eq!(plan.members.len(), 2);
    assert_eq!(plan.members[0].source_name, "node-a.E01");
    assert_eq!(plan.members[0].member_index, 0);
    assert_eq!(plan.members[1].source_name, "node-b.raw");
    assert!(plan.manifest_rel_path.starts_with("clusters/"));
}

#[test]
fn linux_cluster_import_plan_discovers_nested_node_images() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server01 = tmp.path().join("server01");
    let server02 = tmp.path().join("server02");
    std::fs::create_dir_all(&server01).unwrap();
    std::fs::create_dir_all(&server02).unwrap();
    std::fs::write(server01.join("server01-disk01.E01"), b"e01").unwrap();
    std::fs::write(server01.join("server01-disk02.E01"), b"e01").unwrap();
    std::fs::write(server01.join("server01-disk02.E02"), b"segment").unwrap();
    std::fs::write(server02.join("server02-disk01.raw"), b"raw").unwrap();

    let plan = plan_linux_cluster_import(tmp.path(), Some("pve-cluster".to_string())).unwrap();

    let member_paths = plan
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
        member_paths,
        vec![
            "server01/server01-disk01.E01",
            "server01/server01-disk02.E01",
            "server02/server02-disk01.raw"
        ]
    );
    assert_eq!(plan.members[0].member_index, 0);
    assert_eq!(plan.members[1].member_index, 1);
    assert_eq!(plan.members[2].member_index, 2);
}

#[test]
fn linux_cluster_import_plan_requires_multiple_images() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("single.raw"), b"raw").unwrap();

    let err = plan_linux_cluster_import(tmp.path(), None).unwrap_err();

    assert!(matches!(err, ClusterServiceError::InsufficientSources));
}

#[test]
fn linux_cluster_member_configs_are_linux_scoped() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
    let plan = plan_linux_cluster_import(tmp.path(), None).unwrap();

    let configs = plan.member_import_configs();

    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].platform, DataSourcePlatform::Linux);
    assert_eq!(
        configs[0]
            .cluster
            .as_ref()
            .map(|value| value.cluster_id.as_str()),
        Some(plan.cluster_id.as_str())
    );
    assert_eq!(configs[0].cluster.as_ref().unwrap().member_count, 2);
}

#[test]
fn linux_cluster_import_plan_normalizes_profile_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();

    let plan =
        plan_linux_cluster_import(tmp.path(), Some("  pve-audit  ".to_string())).expect("plan");

    assert_eq!(plan.cluster_name, "pve-audit");
    assert_eq!(plan.profile.as_deref(), Some("pve-audit"));
}

#[test]
fn linux_cluster_manifest_write_is_atomic_and_readable() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.raw"), b"raw").unwrap();
    std::fs::write(tmp.path().join("b.raw"), b"raw").unwrap();
    let plan = plan_linux_cluster_import(tmp.path(), None).expect("plan");
    let case_root = tempfile::TempDir::new().unwrap();

    let manifest_path = write_linux_cluster_manifest(case_root.path(), &plan).expect("write");

    assert!(manifest_path.exists());
    assert!(!manifest_path.with_extension("json.tmp").exists());
    let manifest = std::fs::read_to_string(manifest_path).expect("manifest");
    assert!(manifest.contains(&plan.cluster_id));
    assert!(manifest.contains("\"memberCount\": 2"));
}

#[test]
fn linux_cluster_coverage_report_is_atomic_and_redacts_host_paths() {
    let case_root = tempfile::TempDir::new().unwrap();
    let report = crate::ceph_reconstruction::InventoryCoverageReport {
        policy: crate::ceph_reconstruction::RbdReplicaPolicy::strict_legacy(),
        expected_count: 3,
        observed_count: 2,
        state: crate::ceph_reconstruction::InventoryCoverageState::Incomplete,
        duplicate_inventory_ids: Vec::new(),
        duplicate_source_ids: Vec::new(),
        duplicate_osd_ids: Vec::new(),
        ceph_fsids: vec!["fsid".to_string()],
        diagnostics: vec!["inventory coverage is not closed".to_string()],
    };

    let path = write_linux_cluster_coverage_report(case_root.path(), "cluster-1", &report)
        .expect("write coverage report");
    let payload = std::fs::read_to_string(path).expect("coverage report");

    assert!(payload.contains("strict_rbd_replica_count"));
    assert!(payload.contains("inventory coverage is not closed"));
    assert!(!payload.contains("source.db"));
    assert!(!case_root
        .path()
        .join("clusters/cluster-1/coverage-report.json.tmp")
        .exists());
}

#[test]
fn linux_cluster_coverage_report_rejects_path_components() {
    let case_root = tempfile::TempDir::new().unwrap();
    let report = crate::ceph_reconstruction::InventoryCoverageReport {
        policy: crate::ceph_reconstruction::RbdReplicaPolicy::strict_legacy(),
        expected_count: 0,
        observed_count: 0,
        state: crate::ceph_reconstruction::InventoryCoverageState::Indeterminate,
        duplicate_inventory_ids: Vec::new(),
        duplicate_source_ids: Vec::new(),
        duplicate_osd_ids: Vec::new(),
        ceph_fsids: Vec::new(),
        diagnostics: Vec::new(),
    };

    let error = write_linux_cluster_coverage_report(case_root.path(), "../escape", &report)
        .expect_err("path traversal must be rejected");
    assert!(matches!(error, ClusterServiceError::InvalidClusterId));
}

#[test]
fn linux_cluster_coverage_report_round_trips_and_missing_is_empty() {
    let case_root = tempfile::TempDir::new().unwrap();
    assert!(
        read_linux_cluster_coverage_report(case_root.path(), "cluster-1")
            .expect("missing report")
            .is_none()
    );
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
    write_linux_cluster_coverage_report(case_root.path(), "cluster-1", &report)
        .expect("write report");
    assert_eq!(
        read_linux_cluster_coverage_report(case_root.path(), "cluster-1").expect("read report"),
        Some(report)
    );
}

#[test]
fn linux_cluster_coverage_report_round_trips_trusted_pool_policy() {
    let case_root = tempfile::TempDir::new().unwrap();
    let policy = crate::ceph_reconstruction::RbdReplicaPolicy::trusted_pool(
        8,
        2,
        1,
        "osdmap:epoch-17",
        Some(17),
        None,
    )
    .expect("policy");
    let report = crate::ceph_reconstruction::assess_inventory_coverage(&[], &policy);
    write_linux_cluster_coverage_report(case_root.path(), "cluster-1", &report)
        .expect("write report");

    let restored = read_linux_cluster_coverage_report(case_root.path(), "cluster-1")
        .expect("read report")
        .expect("report exists");
    assert_eq!(restored, report);
    assert_eq!(restored.policy.storage_key(), "trusted_pool_size");
}

#[test]
fn linux_cluster_coverage_report_accepts_schema_one_strict_legacy_payload() {
    let case_root = tempfile::TempDir::new().unwrap();
    let path = case_root
        .path()
        .join("clusters/cluster-1/coverage-report.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        r#"{"schemaVersion":1,"clusterId":"cluster-1","evidenceKind":"rbd_osd_inventory","coveragePolicy":"strict_rbd_replica_count","report":{"expectedCount":3,"observedCount":0,"state":"incomplete","duplicateInventoryIds":[],"duplicateSourceIds":[],"duplicateOsdIds":[],"cephFsids":[],"diagnostics":[]}}"#,
    )
    .unwrap();

    let report = read_linux_cluster_coverage_report(case_root.path(), "cluster-1")
        .expect("read legacy report")
        .expect("legacy report exists");
    assert_eq!(report.policy.storage_key(), "strict_rbd_replica_count");
    assert_eq!(report.expected_count, 3);
}

#[test]
fn linux_cluster_coverage_report_rejects_tampering() {
    let case_root = tempfile::TempDir::new().unwrap();
    let path = case_root
        .path()
        .join("clusters/cluster-1/coverage-report.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        r#"{"schemaVersion":1,"clusterId":"cluster-2","evidenceKind":"rbd_osd_inventory","coveragePolicy":"strict_rbd_replica_count","report":{"expectedCount":1,"observedCount":1,"state":"complete","duplicateInventoryIds":[],"duplicateSourceIds":[],"duplicateOsdIds":[],"cephFsids":[],"diagnostics":[]}}"#,
    )
    .unwrap();

    let error = read_linux_cluster_coverage_report(case_root.path(), "cluster-1")
        .expect_err("tampered cluster identity must fail");
    assert!(matches!(error, ClusterServiceError::InvalidCoverageReport));
}

#[test]
fn linux_cluster_coverage_report_rejects_schema_three_payload_tampering() {
    let case_root = tempfile::TempDir::new().unwrap();
    let report = crate::ceph_reconstruction::InventoryCoverageReport {
        policy: crate::ceph_reconstruction::RbdReplicaPolicy::strict_legacy(),
        expected_count: 3,
        observed_count: 0,
        state: crate::ceph_reconstruction::InventoryCoverageState::Incomplete,
        duplicate_inventory_ids: Vec::new(),
        duplicate_source_ids: Vec::new(),
        duplicate_osd_ids: Vec::new(),
        ceph_fsids: Vec::new(),
        diagnostics: vec!["inventory coverage is not closed".to_string()],
    };
    let path = write_linux_cluster_coverage_report(case_root.path(), "cluster-1", &report)
        .expect("write report");
    let mut payload: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    payload["report"]["diagnostics"] = serde_json::json!(["tampered"]);
    std::fs::write(
        case_root
            .path()
            .join("clusters/cluster-1/coverage-report.json"),
        serde_json::to_vec(&payload).unwrap(),
    )
    .unwrap();

    let error = read_linux_cluster_coverage_report(case_root.path(), "cluster-1")
        .expect_err("digest must detect report tampering");
    assert!(matches!(error, ClusterServiceError::InvalidCoverageReport));
}
