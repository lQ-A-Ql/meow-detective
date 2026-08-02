use super::*;
use crate::migrations::runner;
use domain::{EdgeType, NodeType};
use rusqlite::Connection;

fn node(id: &str) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        case_id: "case-1".to_string(),
        node_type: NodeType::Entity,
        label: id.to_string(),
        summary: String::new(),
        tags: vec!["entity".to_string()],
        created_at: "2026-08-02T00:00:00Z".to_string(),
    }
}

#[test]
fn projection_replacement_is_atomic_and_readable() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    runner::run_case_graph_all(&connection).unwrap();
    let projection = CaseGraphProjection {
        case_id: "case-1".to_string(),
        projection_version: "case-graph-v1".to_string(),
        source_manifest: "manifest-1".to_string(),
        built_at: "2026-08-02T00:00:00Z".to_string(),
        source_count: 2,
        cross_source_entity_count: 1,
        cross_source_edge_count: 1,
        seed_ids: vec!["case:entity:one".to_string()],
    };
    let source = CaseGraphSourceState {
        data_source_id: "source-1".to_string(),
        schema_version: "source-029".to_string(),
        database_size_bytes: 42,
        database_modified_ns: "100".to_string(),
        wal_size_bytes: 0,
        wal_modified_ns: "0".to_string(),
    };
    let nodes = vec![node("source:entity"), node("case:entity:one")];
    let edges = vec![GraphEdge {
        id: "case:edge:one".to_string(),
        case_id: "case-1".to_string(),
        source_id: "source:entity".to_string(),
        target_id: "case:entity:one".to_string(),
        edge_type: EdgeType::CorrelatesWith,
        confidence: Some(1.0),
        provenance: None,
        created_at: "2026-08-02T00:00:00Z".to_string(),
    }];

    let repo = CaseGraphRepo::new(&connection);
    repo.replace_projection(&projection, &[source], &nodes, &edges)
        .unwrap();

    assert_eq!(repo.get_projection().unwrap(), Some(projection.clone()));
    assert!(GraphRepo::new(&connection)
        .get_node("case:entity:one")
        .unwrap()
        .is_some());

    let invalid_edges = vec![GraphEdge {
        id: "case:edge:invalid".to_string(),
        case_id: "case-1".to_string(),
        source_id: "missing".to_string(),
        target_id: "case:entity:one".to_string(),
        edge_type: EdgeType::CorrelatesWith,
        confidence: Some(1.0),
        provenance: None,
        created_at: "2026-08-02T00:00:00Z".to_string(),
    }];
    assert!(repo
        .replace_projection(&projection, &[], &nodes, &invalid_edges)
        .is_err());
    assert_eq!(repo.get_projection().unwrap(), Some(projection));
    assert!(GraphRepo::new(&connection)
        .get_node("case:entity:one")
        .unwrap()
        .is_some());
}

#[test]
fn case_graph_schema_reopens_read_only_with_required_indexes() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("indexes").join("case-graph.db");
    let writer = crate::open_or_create_case_graph(&path).unwrap();
    let index_count: i64 = writer
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index'
               AND name IN (
                   'idx_case_graph_nodes_case_type',
                   'idx_case_graph_edges_source',
                   'idx_case_graph_edges_target'
               )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 3);
    drop(writer);

    let reader = crate::open_existing_case_graph_read_only(&path).unwrap();
    let query_only: i64 = reader
        .query_row("PRAGMA query_only", [], |row| row.get(0))
        .unwrap();
    assert_eq!(query_only, 1);
    assert!(reader
        .execute("INSERT INTO case_graph_projection (singleton, case_id, projection_version, source_manifest, built_at, source_count, cross_source_entity_count, cross_source_edge_count, seed_ids_json) VALUES (1, '', '', '', '', 0, 0, 0, '[]')", [])
        .is_err());
}
