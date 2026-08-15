use super::*;
use domain::{DataSourceKind, DataSourcePlatform};
use std::path::PathBuf;

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
