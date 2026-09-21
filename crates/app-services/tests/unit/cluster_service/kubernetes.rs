use crate::cluster_service::{
    discover_kubernetes_cluster_artifacts, discover_kubernetes_member_artifacts, TopologyScopeKind,
};
use domain::{CaseId, DataSourceId, EntryType, FileEntry, FileEntryId, KubernetesScopeId};
use persistence_sqlite::repositories::{
    file_repo::FileRepo,
    linux_topology_scope_repo::{LinuxTopologyScopeRecord, LinuxTopologyScopeRepo},
};

#[test]
fn member_inventory_reads_source_catalog_without_writing_it() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_id = DataSourceId("k8s-source".to_string());
    let connection =
        persistence_sqlite::open_or_create_source(&tmp.path().join("source.db")).unwrap();
    FileRepo::new(&connection)
        .insert_batch(&[FileEntry {
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
        }])
        .unwrap();

    let inventory = discover_kubernetes_member_artifacts(&connection, &source_id).unwrap();
    assert_eq!(inventory.data_source_id, source_id);
    assert_eq!(inventory.scanned_entries, 1);
    assert_eq!(inventory.artifacts.len(), 1);
    assert_eq!(inventory.artifacts[0].file_id.0, "file-manifest");
}

#[test]
fn cluster_inventory_rejects_a_non_kubernetes_topology_scope() {
    let connection = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::migrations::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-k8s', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    LinuxTopologyScopeRepo::new(&connection)
        .insert(&LinuxTopologyScopeRecord {
            id: "scope:pve:1".to_string(),
            case_id: "case-k8s".to_string(),
            scope_kind: TopologyScopeKind::Pve.as_str().to_string(),
            name: "PVE".to_string(),
            identity_state: "unproven".to_string(),
            identity_fingerprint: None,
            status: "ready".to_string(),
            evidence_completeness: "complete".to_string(),
            diagnostics_json: "[]".to_string(),
        })
        .unwrap();

    let error = discover_kubernetes_cluster_artifacts(
        &connection,
        std::path::Path::new("D:/case"),
        &CaseId("case-k8s".to_string()),
        &KubernetesScopeId("scope:pve:1".to_string()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::cluster_service::ClusterServiceError::InvalidClusterId
    ));
}
