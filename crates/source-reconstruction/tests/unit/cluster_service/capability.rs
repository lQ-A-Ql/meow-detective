use crate::cluster_service::{require_ceph_scope, require_kubernetes_scope};
use domain::{CaseId, CephScopeId, KubernetesScopeId};
use persistence_sqlite::repositories::linux_topology_scope_repo::{
    LinuxTopologyScopeRecord, LinuxTopologyScopeRepo,
};

#[test]
fn typed_capability_guards_reject_cross_layer_scope_ids() {
    let connection = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::migrations::runner::run_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-capability', 'capability')",
            [],
        )
        .unwrap();
    connection.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-capability', 'case-capability', 'source', 'raw', 'source.raw', datetime('now'))",
        [],
    ).unwrap();
    let repo = LinuxTopologyScopeRepo::new(&connection);
    repo.insert(&LinuxTopologyScopeRecord {
        id: "scope:kubernetes:1".to_string(),
        case_id: "case-capability".to_string(),
        scope_kind: "kubernetes".to_string(),
        name: "Kubernetes".to_string(),
        identity_state: "candidate".to_string(),
        identity_fingerprint: None,
        status: "ready".to_string(),
        evidence_completeness: "complete".to_string(),
        diagnostics_json: "[]".to_string(),
    })
    .unwrap();
    connection.execute(
        "INSERT INTO linux_topology_memberships (scope_id, data_source_id, role, confidence, provenance_json)
         VALUES ('scope:kubernetes:1', 'source-capability', 'control_plane', 'candidate', '{}')",
        [],
    ).unwrap();
    assert!(require_ceph_scope(
        &connection,
        &CaseId("case-capability".to_string()).0,
        &CephScopeId("scope:kubernetes:1".to_string()),
    )
    .is_err());
    assert!(require_kubernetes_scope(
        &connection,
        "case-capability",
        &KubernetesScopeId("scope:kubernetes:1".to_string()),
    )
    .is_ok());
}
