use super::*;
use domain::{CaseId, CaseMeta, EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{case_repo::CaseRepo, graph_repo::GraphRepo};
use rusqlite::Connection;

fn setup_case_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: CaseId("case-1".to_string()),
            name: "Graph Test Case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    conn
}

fn make_node(id: &str, case_id: &str, node_type: NodeType, label: &str) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        case_id: case_id.to_string(),
        node_type,
        label: label.to_string(),
        summary: format!("Summary for {id}"),
        tags: vec!["test".to_string()],
        created_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

fn make_edge(
    id: &str,
    case_id: &str,
    source: &str,
    target: &str,
    edge_type: EdgeType,
) -> GraphEdge {
    GraphEdge {
        id: id.to_string(),
        case_id: case_id.to_string(),
        source_id: source.to_string(),
        target_id: target.to_string(),
        edge_type,
        confidence: Some(0.95),
        provenance: None,
        created_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

#[test]
fn snapshot_empty_graph() {
    let conn = setup_case_db();
    let snapshot = get_graph_snapshot(&conn, "case-1").unwrap();
    assert_eq!(snapshot.total_nodes, 0);
    assert_eq!(snapshot.total_edges, 0);
    assert_eq!(snapshot.density, 0.0);
    assert_eq!(snapshot.largest_component_size, 0);
}

#[test]
fn snapshot_with_nodes_and_edges() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "a.exe"),
        make_node("n2", "case-1", NodeType::File, "b.dll"),
        make_node("n3", "case-1", NodeType::Artifact, "LNK-1"),
    ])
    .unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
        make_edge("e2", "case-1", "n1", "n3", EdgeType::References),
    ])
    .unwrap();

    let snapshot = get_graph_snapshot(&conn, "case-1").unwrap();
    assert_eq!(snapshot.total_nodes, 3);
    assert_eq!(snapshot.total_edges, 2);
    assert!(snapshot.density > 0.0);
    assert!(
        snapshot
            .node_count_by_type
            .get("file")
            .copied()
            .unwrap_or(0)
            >= 2
    );
}

#[test]
fn query_graph_traversal() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "a.exe"),
        make_node("n2", "case-1", NodeType::File, "b.dll"),
        make_node("n3", "case-1", NodeType::Artifact, "LNK-1"),
    ])
    .unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
        make_edge("e2", "case-1", "n2", "n3", EdgeType::References),
    ])
    .unwrap();

    let result = query_graph(
        &conn,
        GraphQueryDto {
            start_ids: vec!["n1".to_string()],
            edge_types: vec![],
            max_depth: 2,
            confidence_floor: None,
            limit: 100,
        },
    )
    .unwrap();

    assert_eq!(result.node_count, 3);
    assert_eq!(result.edge_count, 2);
    assert!(result.nodes.iter().any(|n| n.id == "n1"));
    assert!(result.nodes.iter().any(|n| n.id == "n3"));
}

#[test]
fn query_graph_respects_confidence_floor() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "a.exe"),
        make_node("n2", "case-1", NodeType::File, "b.dll"),
    ])
    .unwrap();

    let mut high_confidence = make_edge("e1", "case-1", "n1", "n2", EdgeType::References);
    high_confidence.confidence = Some(0.9);
    repo.insert_edges_batch(&[high_confidence]).unwrap();

    let result = query_graph(
        &conn,
        GraphQueryDto {
            start_ids: vec!["n1".to_string()],
            edge_types: vec![],
            max_depth: 2,
            confidence_floor: Some(0.5),
            limit: 100,
        },
    )
    .unwrap();

    assert_eq!(result.edge_count, 1);

    let result_strict = query_graph(
        &conn,
        GraphQueryDto {
            start_ids: vec!["n1".to_string()],
            edge_types: vec![],
            max_depth: 2,
            confidence_floor: Some(0.99),
            limit: 100,
        },
    )
    .unwrap();

    assert_eq!(result_strict.edge_count, 0);
}

#[test]
fn list_graph_nodes_returns_case_nodes() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "a.exe"),
        make_node("n2", "case-1", NodeType::Artifact, "LNK-1"),
    ])
    .unwrap();

    let nodes = list_graph_nodes(
        &conn,
        "case-1",
        ListGraphNodesRequest {
            limit: 10,
            offset: 0,
        },
    )
    .unwrap();

    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|node| node.id == "n1"));
    assert!(nodes.iter().any(|node| node.id == "n2"));
}

#[test]
fn node_neighborhood_returns_connected_subgraph() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "center.exe"),
        make_node("n2", "case-1", NodeType::File, "neighbor-a.dll"),
        make_node("n3", "case-1", NodeType::File, "neighbor-b.dll"),
        make_node("n4", "case-1", NodeType::File, "far-away.exe"),
    ])
    .unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
        make_edge("e2", "case-1", "n3", "n1", EdgeType::Contains),
        make_edge("e3", "case-1", "n2", "n4", EdgeType::References),
    ])
    .unwrap();

    let result = get_node_neighborhood(&conn, "n1", 1).unwrap();
    // depth 1: n1 itself + n2 (outgoing) + n3 (incoming) = 3 nodes
    assert_eq!(result.node_count, 3);
    // n4 should NOT be included at depth 1
    assert!(!result.nodes.iter().any(|n| n.id == "n4"));
}

#[test]
fn case_graph_single_node_lookup_rejects_unscoped_ids() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = setup_case_db();

    let err = get_node_neighborhood_for_case(&conn, tmp.path(), "case-1", "n1", 1).unwrap_err();

    assert!(matches!(err, GraphServiceError::InvalidInput(_)));
    assert!(err.to_string().contains("ds:<dataSourceId>:<localId>"));
}

#[test]
fn case_graph_provenance_lookup_rejects_unscoped_ids() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = setup_case_db();

    let err = get_provenance_chain_for_case(&conn, tmp.path(), "case-1", "e1").unwrap_err();

    assert!(matches!(err, GraphServiceError::InvalidInput(_)));
    assert!(err.to_string().contains("ds:<dataSourceId>:<localId>"));
}

#[test]
fn case_graph_query_start_ids_reject_unscoped_ids() {
    let err = scoped_start_ids(&["n1".to_string()]).unwrap_err();

    assert!(matches!(err, GraphServiceError::InvalidInput(_)));
    assert!(err.to_string().contains("ds:<dataSourceId>:<localId>"));
}

#[test]
fn provenance_chain_without_provenance_metadata() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "a.exe"),
        make_node("n2", "case-1", NodeType::File, "b.dll"),
    ])
    .unwrap();
    repo.insert_edges_batch(&[make_edge("e1", "case-1", "n1", "n2", EdgeType::References)])
        .unwrap();

    let chain = get_provenance_chain(&conn, "e1").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].edge_id, "e1");
    assert!(chain[0].source_rule_id.is_none());
    assert!(chain[0].extraction_timestamp.is_some());
}

#[test]
fn provenance_chain_with_correlation_provenance() {
    let conn = setup_case_db();
    let repo = GraphRepo::new(&conn);

    repo.insert_nodes_batch(&[
        make_node("n1", "case-1", NodeType::File, "cmd.exe"),
        make_node("n2", "case-1", NodeType::Artifact, "LNK Artifact"),
    ])
    .unwrap();

    let provenance = serde_json::json!({
        "kind": "correlation_rule",
        "lead_id": "lead:rules:file-cmd",
        "match_signals": ["LNK 目标路径命中文件路径"],
        "families": ["LNK"]
    })
    .to_string();

    let mut edge = make_edge("e1", "case-1", "n2", "n1", EdgeType::CorrelatesWith);
    edge.provenance = Some(provenance);
    repo.insert_edges_batch(&[edge]).unwrap();

    let chain = get_provenance_chain(&conn, "e1").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].edge_id, "e1");
    assert_eq!(chain[0].source_parser.as_deref(), Some("LNK"));
}
