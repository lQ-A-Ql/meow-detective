use super::*;
use persistence_sqlite::connection::open_in_memory;
use persistence_sqlite::migrations::runner;

fn setup() -> (&'static Connection, String) {
    let conn = Box::new(open_in_memory().unwrap());
    let conn_ref: &'static Connection = Box::leak(conn);
    runner::run_all(conn_ref).unwrap();
    let case_id = "case-gql-1".to_string();
    conn_ref
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, 'Test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![case_id],
        )
        .unwrap();

    let repo = GraphRepo::new(conn_ref);
    repo.insert_nodes_batch(&[
        GraphNode {
            id: "f1".to_string(),
            case_id: case_id.clone(),
            node_type: NodeType::File,
            label: "cmd.exe".to_string(),
            summary: "Command Prompt".to_string(),
            tags: vec!["executable".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        GraphNode {
            id: "f2".to_string(),
            case_id: case_id.clone(),
            node_type: NodeType::File,
            label: "powershell.exe".to_string(),
            summary: "PowerShell".to_string(),
            tags: vec!["executable".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        GraphNode {
            id: "a1".to_string(),
            case_id: case_id.clone(),
            node_type: NodeType::Artifact,
            label: "LNK-1".to_string(),
            summary: "A shell link file".to_string(),
            tags: vec!["lnk".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        GraphNode {
            id: "a2".to_string(),
            case_id: case_id.clone(),
            node_type: NodeType::Artifact,
            label: "Prefetch-1".to_string(),
            summary: "A prefetch file".to_string(),
            tags: vec!["prefetch".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
    ])
    .unwrap();
    repo.insert_edges_batch(&[
        GraphEdge {
            id: "e1".to_string(),
            case_id: case_id.clone(),
            source_id: "f1".to_string(),
            target_id: "a1".to_string(),
            edge_type: EdgeType::References,
            confidence: Some(0.95),
            provenance: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        GraphEdge {
            id: "e2".to_string(),
            case_id: case_id.clone(),
            source_id: "f1".to_string(),
            target_id: "a2".to_string(),
            edge_type: EdgeType::References,
            confidence: Some(0.60),
            provenance: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        GraphEdge {
            id: "e3".to_string(),
            case_id: case_id.clone(),
            source_id: "f2".to_string(),
            target_id: "a1".to_string(),
            edge_type: EdgeType::References,
            confidence: Some(0.80),
            provenance: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
    ])
    .unwrap();
    (conn_ref, case_id)
}

#[test]
fn engine_match_file_to_artifact() {
    let (conn, case_id) = setup();
    let query =
        crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m").unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.source_nodes.len(), 3);
    assert_eq!(result.edges.len(), 3);
    assert_eq!(result.target_nodes.len(), 3);
    assert_eq!(result.total_matched, 3);
}

#[test]
fn engine_where_confidence_filter() {
    let (conn, case_id) = setup();
    let query = crate::parser::parse(
        "MATCH (n:File)-[e:References]->(m:Artifact) WHERE e.confidence > 0.7 RETURN n, e, m",
    )
    .unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.total_matched, 2);
}

#[test]
fn engine_where_label_filter() {
    let (conn, case_id) = setup();
    let query = crate::parser::parse(
        "MATCH (n:File)-[e:References]->(m:Artifact) WHERE n.label = 'cmd.exe' RETURN n, e, m",
    )
    .unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.total_matched, 2);
    assert_eq!(result.source_nodes[0].label, "cmd.exe");
}

#[test]
fn engine_limit() {
    let (conn, case_id) = setup();
    let query =
        crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m LIMIT 1")
            .unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.source_nodes.len(), 1);
    assert_eq!(result.total_matched, 3);
}

#[test]
fn engine_count_aggregate() {
    let (conn, case_id) = setup();
    let query = crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN count(*)")
        .unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.aggregates.get("count(*)"), Some(&3.0));
}

#[test]
fn engine_reverse_direction() {
    let (conn, case_id) = setup();
    let query =
        crate::parser::parse("MATCH (a:Artifact)<-[e:References]-(f:File) RETURN a, e, f").unwrap();
    let result = GqlEngine::new(conn).execute(&case_id, &query).unwrap();
    assert_eq!(result.total_matched, 3);
    assert!(result
        .source_nodes
        .iter()
        .all(|node| matches!(node.node_type, NodeType::Artifact)));
}
