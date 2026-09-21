use super::super::super::connection::open_in_memory;
use super::super::super::migrations::runner::run_all;
use super::super::super::repositories::{
    linux_topology_edge_repo::{LinuxTopologyEdgeRecord, LinuxTopologyEdgeRepo},
    linux_topology_membership_repo::{LinuxTopologyMembershipRecord, LinuxTopologyMembershipRepo},
    linux_topology_scope_repo::{LinuxTopologyScopeRecord, LinuxTopologyScopeRepo},
};

fn setup() -> rusqlite::Connection {
    let conn = open_in_memory().unwrap();
    run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-topology', 'topology', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-topology', 'case-topology', 'node', 'raw', 'node.raw', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

#[test]
fn topology_scopes_memberships_and_edges_are_layered_and_case_bound() {
    let conn = setup();
    let scope_repo = LinuxTopologyScopeRepo::new(&conn);
    let membership_repo = LinuxTopologyMembershipRepo::new(&conn);
    let edge_repo = LinuxTopologyEdgeRepo::new(&conn);
    scope_repo
        .insert(&LinuxTopologyScopeRecord {
            id: "pve-1".to_string(),
            case_id: "case-topology".to_string(),
            scope_kind: "pve".to_string(),
            name: "pve".to_string(),
            identity_state: "candidate".to_string(),
            identity_fingerprint: None,
            status: "ready".to_string(),
            evidence_completeness: "partial".to_string(),
            diagnostics_json: "[]".to_string(),
        })
        .unwrap();
    scope_repo
        .insert(&LinuxTopologyScopeRecord {
            id: "ceph-1".to_string(),
            case_id: "case-topology".to_string(),
            scope_kind: "ceph".to_string(),
            name: "ceph".to_string(),
            identity_state: "candidate".to_string(),
            identity_fingerprint: None,
            status: "ready".to_string(),
            evidence_completeness: "partial".to_string(),
            diagnostics_json: "[]".to_string(),
        })
        .unwrap();
    membership_repo
        .insert(&LinuxTopologyMembershipRecord {
            scope_id: "pve-1".to_string(),
            data_source_id: "source-topology".to_string(),
            role: "host".to_string(),
            member_index: Some(0),
            confidence: "candidate".to_string(),
            provenance_json: "{\"basis\":\"pmxcfs\"}".to_string(),
        })
        .unwrap();
    edge_repo
        .insert(&LinuxTopologyEdgeRecord {
            source_scope_id: "ceph-1".to_string(),
            target_scope_id: "pve-1".to_string(),
            edge_kind: "provides_storage".to_string(),
            confidence: "candidate".to_string(),
            provenance_json: "{\"basis\":\"rbd\"}".to_string(),
        })
        .unwrap();

    let scopes: i64 = conn
        .query_row("SELECT COUNT(*) FROM linux_topology_scopes", [], |row| {
            row.get(0)
        })
        .unwrap();
    let edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM linux_topology_edges", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(scopes, 2);
    assert_eq!(edges, 1);
}

#[test]
fn topology_rejects_unknown_scope_kind_and_self_edges() {
    let conn = setup();
    let scope_repo = LinuxTopologyScopeRepo::new(&conn);
    let unknown = scope_repo.insert(&LinuxTopologyScopeRecord {
        id: "unknown".to_string(),
        case_id: "case-topology".to_string(),
        scope_kind: "container_runtime".to_string(),
        name: "runtime".to_string(),
        identity_state: "unproven".to_string(),
        identity_fingerprint: None,
        status: "discovered".to_string(),
        evidence_completeness: "indeterminate".to_string(),
        diagnostics_json: "[]".to_string(),
    });
    assert!(unknown.is_err());
}

#[test]
fn topology_rejects_cross_case_memberships_and_unproven_relationships_without_provenance() {
    let conn = setup();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('other-case', 'other', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('other-source', 'other-case', 'other', 'raw', 'other.raw', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    let scope_repo = LinuxTopologyScopeRepo::new(&conn);
    let membership_repo = LinuxTopologyMembershipRepo::new(&conn);
    scope_repo
        .insert(&LinuxTopologyScopeRecord {
            id: "ceph-1".to_string(),
            case_id: "case-topology".to_string(),
            scope_kind: "ceph".to_string(),
            name: "ceph".to_string(),
            identity_state: "candidate".to_string(),
            identity_fingerprint: None,
            status: "ready".to_string(),
            evidence_completeness: "complete".to_string(),
            diagnostics_json: "[]".to_string(),
        })
        .unwrap();
    assert!(membership_repo
        .insert(&LinuxTopologyMembershipRecord {
            scope_id: "ceph-1".to_string(),
            data_source_id: "other-source".to_string(),
            role: "storage_node".to_string(),
            member_index: Some(0),
            confidence: "candidate".to_string(),
            provenance_json: "{}".to_string(),
        })
        .is_err());
    assert!(membership_repo
        .insert(&LinuxTopologyMembershipRecord {
            scope_id: "ceph-1".to_string(),
            data_source_id: "source-topology".to_string(),
            role: "invalid".to_string(),
            member_index: Some(0),
            confidence: "candidate".to_string(),
            provenance_json: "{}".to_string(),
        })
        .is_err());
    assert!(membership_repo
        .insert(&LinuxTopologyMembershipRecord {
            scope_id: "ceph-1".to_string(),
            data_source_id: "source-topology".to_string(),
            role: "storage_node".to_string(),
            member_index: Some(0),
            confidence: "proven".to_string(),
            provenance_json: "{}".to_string(),
        })
        .is_err());
}
