use crate::cluster_service::{
    discover_kubernetes_cluster_artifacts, discover_kubernetes_member_artifacts,
    plan_kubernetes_cluster_import, KubernetesClusterPlan, KUBERNETES_CLUSTER_PROFILE,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::{
    datasource_cluster_repo::{DataSourceClusterRecord, DataSourceClusterRepo},
    file_repo::FileRepo,
};

#[test]
fn kubernetes_plan_reuses_linux_member_scheduler_and_sets_profile() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("node-a.E01"), b"e01").unwrap();
    std::fs::write(tmp.path().join("node-b.raw"), b"raw").unwrap();

    let plan = plan_kubernetes_cluster_import(tmp.path(), Some("cluster-a".to_string()))
        .expect("kubernetes cluster plan");

    assert_eq!(
        plan.import.profile.as_deref(),
        Some(KUBERNETES_CLUSTER_PROFILE)
    );
    assert_eq!(plan.members().len(), 2);
    assert_eq!(plan.member_import_configs().len(), 2);
    assert!(matches!(plan, KubernetesClusterPlan { .. }));
}

#[test]
fn member_inventory_reads_source_catalog_without_writing_it() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_id = DataSourceId("k8s-source".to_string());
    let connection = persistence_sqlite::open_or_create_source(&tmp.path().join("source.db"))
        .expect("source db");
    let entries = [
        FileEntry {
            id: FileEntryId("file-manifest".to_string()),
            parent_id: None,
            data_source_id: source_id.clone(),
            path: "[P2]\\etc\\kubernetes\\manifests\\kube-apiserver.yaml".to_string(),
            name: "kube-apiserver.yaml".to_string(),
            entry_type: EntryType::File,
            size: Some(42),
            ext: Some("yaml".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: true,
            archive: false,
            unix_mode: Some(0o600),
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
        FileEntry {
            id: FileEntryId("directory-only".to_string()),
            parent_id: None,
            data_source_id: source_id.clone(),
            path: "/var/log/containers".to_string(),
            name: "containers".to_string(),
            entry_type: EntryType::Directory,
            size: None,
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: Some(0o755),
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
    ];
    FileRepo::new(&connection)
        .insert_batch(&entries)
        .expect("insert source entries");

    let inventory = discover_kubernetes_member_artifacts(&connection, &source_id)
        .expect("discover Kubernetes artifacts");

    assert_eq!(inventory.data_source_id, source_id);
    assert_eq!(inventory.scanned_entries, 2);
    assert_eq!(inventory.artifacts.len(), 1);
    assert_eq!(inventory.artifacts[0].file_id.0, "file-manifest");
    assert!(!inventory.truncated);
}

#[test]
fn cluster_inventory_rejects_non_kubernetes_profiles() {
    let connection = persistence_sqlite::connection::open_in_memory().expect("case db");
    persistence_sqlite::migrations::runner::run_all(&connection).expect("case migrations");
    connection
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-k8s', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("case row");
    DataSourceClusterRepo::new(&connection)
        .insert_pending(&DataSourceClusterRecord {
            id: "cluster-pve".to_string(),
            case_id: domain::CaseId("case-k8s".to_string()),
            name: "pve".to_string(),
            root_path: "D:/cluster".to_string(),
            platform: "linux".to_string(),
            profile: Some("pve".to_string()),
            manifest_rel_path: "clusters/cluster-pve/cluster-manifest.json".to_string(),
            import_state: "ready".to_string(),
            member_count: 0,
            ready_count: 0,
            failed_count: 0,
            last_error: None,
        })
        .expect("cluster row");

    let error = discover_kubernetes_cluster_artifacts(
        &connection,
        std::path::Path::new("D:/case"),
        &domain::CaseId("case-k8s".to_string()),
        "cluster-pve",
    )
    .expect_err("PVE profile must not enter Kubernetes aggregation");

    assert!(matches!(
        error,
        crate::cluster_service::ClusterServiceError::Unsupported
    ));
}

#[test]
#[ignore = "requires FORENSICS_K8S_CLUSTER_ROOT real Kubernetes cluster sample"]
fn real_kubernetes_cluster_plan_discovers_all_sample_members() {
    let root = std::env::var_os("FORENSICS_K8S_CLUSTER_ROOT")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_K8S_CLUSTER_ROOT");

    let plan = plan_kubernetes_cluster_import(&root, Some("kubernetes-sample".to_string()))
        .expect("plan Kubernetes sample");

    eprintln!(
        "Kubernetes sample members={} profile={:?}",
        plan.members().len(),
        plan.import.profile
    );
    assert_eq!(
        plan.import.profile.as_deref(),
        Some(KUBERNETES_CLUSTER_PROFILE)
    );
    assert!(
        plan.members().len() >= 2,
        "a cluster sample must contain at least two image members"
    );
}
