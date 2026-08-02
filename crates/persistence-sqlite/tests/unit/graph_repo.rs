use super::*;
use crate::{connection::open_in_memory, runner};

fn setup() -> (&'static Connection, GraphRepo<'static>) {
    let conn = Box::new(open_in_memory().unwrap());
    let conn_ref: &'static Connection = Box::leak(conn);
    runner::run_all(conn_ref).unwrap();
    conn_ref
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-1', 'Test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    let repo = GraphRepo::new(conn_ref);
    (conn_ref, repo)
}

fn make_node(id: &str, node_type: NodeType, label: &str) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        case_id: "case-1".to_string(),
        node_type,
        label: label.to_string(),
        summary: format!("Summary for {id}"),
        tags: vec!["test".to_string()],
        created_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

fn make_edge(id: &str, source: &str, target: &str, edge_type: EdgeType) -> GraphEdge {
    GraphEdge {
        id: id.to_string(),
        case_id: "case-1".to_string(),
        source_id: source.to_string(),
        target_id: target.to_string(),
        edge_type,
        confidence: Some(0.95),
        provenance: None,
        created_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

#[test]
fn insert_and_get_node() {
    let (_conn, repo) = setup();
    let node = make_node("n1", NodeType::File, "cmd.exe");
    let count = repo
        .insert_nodes_batch(std::slice::from_ref(&node))
        .unwrap();
    assert_eq!(count, 1);

    let fetched = repo.get_node("n1").unwrap().expect("node should exist");
    assert_eq!(fetched.id, "n1");
    assert_eq!(fetched.node_type, NodeType::File);
    assert_eq!(fetched.label, "cmd.exe");
    assert_eq!(fetched.tags, vec!["test"]);
}

#[test]
fn list_nodes_for_case_paginates_newest_first() {
    let (_conn, repo) = setup();
    let mut older = make_node("n1", NodeType::File, "older.exe");
    older.created_at = "2026-06-14T00:00:00Z".to_string();
    let mut newer = make_node("n2", NodeType::Artifact, "newer");
    newer.created_at = "2026-06-15T00:00:00Z".to_string();
    repo.insert_nodes_batch(&[older, newer]).unwrap();

    let first_page = repo.list_nodes_for_case("case-1", 1, 0).unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].id, "n2");

    let second_page = repo.list_nodes_for_case("case-1", 1, 1).unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].id, "n1");
}

#[test]
fn entity_projection_query_applies_its_sql_limit() {
    let (_conn, repo) = setup();
    repo.insert_nodes_batch(&[
        make_node("entity-c", NodeType::Entity, "c"),
        make_node("entity-a", NodeType::Entity, "a"),
        make_node("entity-b", NodeType::Entity, "b"),
        make_node("file-a", NodeType::File, "file"),
    ])
    .unwrap();

    let nodes = repo
        .list_nodes_by_type_for_case_bounded("case-1", &NodeType::Entity, 2)
        .unwrap();
    assert_eq!(
        nodes.into_iter().map(|node| node.id).collect::<Vec<_>>(),
        vec!["entity-a", "entity-b"]
    );
}

#[test]
fn keyset_pagination_preserves_tie_order_without_duplicates() {
    let (_conn, repo) = setup();
    let timestamp = "2026-06-15T00:00:00Z";
    let mut nodes = [
        make_node("node-c", NodeType::File, "c"),
        make_node("node-a", NodeType::File, "a"),
        make_node("node-b", NodeType::File, "b"),
    ];
    for node in &mut nodes {
        node.created_at = timestamp.to_string();
    }
    repo.insert_nodes_batch(&nodes).unwrap();

    let first = repo.list_nodes_for_case_after("case-1", 2, None).unwrap();
    let cursor = GraphNodePageCursor::from(first.last().unwrap());
    let second = repo
        .list_nodes_for_case_after("case-1", 2, Some(&cursor))
        .unwrap();

    assert_eq!(
        first
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["node-a", "node-b"]
    );
    assert_eq!(
        second
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["node-c"]
    );
}

#[test]
fn deep_keyset_seek_uses_composite_ordering_index() {
    const NODE_COUNT: usize = 10_000;
    const DEEP_OFFSET: usize = 9_000;
    let conn = open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    let repo = GraphRepo::new(&conn);
    let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap();
    let nodes = (0..NODE_COUNT)
        .map(|rank| {
            let mut node = make_node(
                &format!("node-{rank:05}"),
                NodeType::File,
                &format!("node-{rank:05}"),
            );
            node.created_at = (base + chrono::Duration::seconds(rank as i64)).to_rfc3339();
            node
        })
        .collect::<Vec<_>>();
    repo.insert_nodes_batch(&nodes).unwrap();

    let cursor_rank = NODE_COUNT - DEEP_OFFSET;
    let cursor = GraphNodePageCursor::from(&nodes[cursor_rank]);
    let page = repo
        .list_nodes_for_case_after("case-1", 25, Some(&cursor))
        .unwrap();
    let expected = (cursor_rank.saturating_sub(25)..cursor_rank)
        .rev()
        .map(|rank| format!("node-{rank:05}"))
        .collect::<Vec<_>>();
    assert_eq!(
        page.into_iter().map(|node| node.id).collect::<Vec<_>>(),
        expected
    );

    let sql = list_nodes_after_sql();
    assert!(!sql.to_ascii_uppercase().contains("OFFSET"));
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain).unwrap();
    let details = stmt
        .query_map(
            params!["case-1", cursor.created_at, cursor.id, 25_u32],
            |row| row.get::<_, String>(3),
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(details.iter().any(|detail| {
        detail.contains("idx_source_graph_nodes_case_created_id") && detail.contains("created_at<?")
    }));
}

#[test]
fn insert_and_get_edge() {
    let (_conn, repo) = setup();
    let n1 = make_node("n1", NodeType::File, "a.exe");
    let n2 = make_node("n2", NodeType::Artifact, "LNK");
    repo.insert_nodes_batch(&[n1, n2]).unwrap();

    let edge = make_edge("e1", "n1", "n2", EdgeType::References);
    let count = repo
        .insert_edges_batch(std::slice::from_ref(&edge))
        .unwrap();
    assert_eq!(count, 1);

    let neighbors = repo.get_neighbors("n1", &[], Direction::Outgoing).unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0.id, "e1");
    assert_eq!(neighbors[0].1.id, "n2");
}

#[test]
fn neighbors_incoming() {
    let (_conn, repo) = setup();
    let n1 = make_node("n1", NodeType::File, "a.exe");
    let n2 = make_node("n2", NodeType::File, "b.exe");
    repo.insert_nodes_batch(&[n1, n2]).unwrap();
    repo.insert_edges_batch(&[make_edge("e1", "n1", "n2", EdgeType::References)])
        .unwrap();

    let neighbors = repo.get_neighbors("n2", &[], Direction::Incoming).unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0.id, "e1");
    assert_eq!(neighbors[0].1.id, "n1");
}

#[test]
fn neighbors_both() {
    let (_conn, repo) = setup();
    let n1 = make_node("n1", NodeType::File, "a.exe");
    let n2 = make_node("n2", NodeType::File, "b.exe");
    let n3 = make_node("n3", NodeType::File, "c.exe");
    repo.insert_nodes_batch(&[n1, n2, n3]).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n3", "n1", EdgeType::Contains),
    ])
    .unwrap();

    let neighbors = repo.get_neighbors("n1", &[], Direction::Both).unwrap();
    assert_eq!(neighbors.len(), 2);
}

#[test]
fn neighbors_filtered_by_edge_type() {
    let (_conn, repo) = setup();
    let n1 = make_node("n1", NodeType::File, "a.exe");
    let n2 = make_node("n2", NodeType::File, "b.exe");
    let n3 = make_node("n3", NodeType::File, "c.exe");
    repo.insert_nodes_batch(&[n1, n2, n3]).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n1", "n3", EdgeType::Contains),
    ])
    .unwrap();

    let refs = repo
        .get_neighbors("n1", &[EdgeType::References], Direction::Outgoing)
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].1.id, "n2");
}

#[test]
fn traverse_bfs_simple_path() {
    let (_conn, repo) = setup();
    let nodes = (1..=4)
        .map(|i| make_node(&format!("n{i}"), NodeType::File, &format!("node{i}")))
        .collect::<Vec<_>>();
    repo.insert_nodes_batch(&nodes).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n2", "n3", EdgeType::References),
        make_edge("e3", "n3", "n4", EdgeType::References),
    ])
    .unwrap();

    let (result_nodes, result_edges) = repo.traverse(&["n1".to_string()], &[], 3, 100).unwrap();
    assert_eq!(result_nodes.len(), 4);
    assert_eq!(result_edges.len(), 3);
}

#[test]
fn traverse_respects_max_depth() {
    let (_conn, repo) = setup();
    let nodes = (1..=5)
        .map(|i| make_node(&format!("n{i}"), NodeType::File, &format!("node{i}")))
        .collect::<Vec<_>>();
    repo.insert_nodes_batch(&nodes).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n2", "n3", EdgeType::References),
        make_edge("e3", "n3", "n4", EdgeType::References),
        make_edge("e4", "n4", "n5", EdgeType::References),
    ])
    .unwrap();

    let (result_nodes, _) = repo.traverse(&["n1".to_string()], &[], 1, 100).unwrap();
    assert_eq!(result_nodes.len(), 2);
}

#[test]
fn traverse_respects_limit() {
    let (_conn, repo) = setup();
    let nodes = (1..=5)
        .map(|i| make_node(&format!("n{i}"), NodeType::File, &format!("node{i}")))
        .collect::<Vec<_>>();
    repo.insert_nodes_batch(&nodes).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n2", "n3", EdgeType::References),
    ])
    .unwrap();

    let (result_nodes, _) = repo.traverse(&["n1".to_string()], &[], 5, 2).unwrap();
    assert_eq!(result_nodes.len(), 2);
}

#[test]
fn snapshot_counts_by_type() {
    let (_conn, repo) = setup();
    let nodes = vec![
        make_node("n1", NodeType::File, "a.exe"),
        make_node("n2", NodeType::File, "b.dll"),
        make_node("n3", NodeType::Artifact, "LNK-1"),
    ];
    repo.insert_nodes_batch(&nodes).unwrap();
    repo.insert_edges_batch(&[
        make_edge("e1", "n1", "n2", EdgeType::References),
        make_edge("e2", "n1", "n3", EdgeType::References),
    ])
    .unwrap();

    let snapshot = repo.get_snapshot("case-1").unwrap();
    assert_eq!(snapshot.total_nodes, 3);
    assert_eq!(snapshot.total_edges, 2);
    assert_eq!(snapshot.node_count_by_type.get("file"), Some(&2));
    assert_eq!(snapshot.node_count_by_type.get("artifact"), Some(&1));
    assert_eq!(snapshot.edge_count_by_type.get("references"), Some(&2));
}

#[test]
fn delete_case_graph_removes_all() {
    let (_conn, repo) = setup();
    let nodes = vec![
        make_node("n1", NodeType::File, "a.exe"),
        make_node("n2", NodeType::File, "b.dll"),
    ];
    repo.insert_nodes_batch(&nodes).unwrap();
    repo.insert_edges_batch(&[make_edge("e1", "n1", "n2", EdgeType::References)])
        .unwrap();

    repo.delete_case_graph("case-1").unwrap();

    let snapshot = repo.get_snapshot("case-1").unwrap();
    assert_eq!(snapshot.total_nodes, 0);
    assert_eq!(snapshot.total_edges, 0);
}

#[test]
fn get_node_nonexistent_returns_none() {
    let (_conn, repo) = setup();
    let result = repo.get_node("no-such-node").unwrap();
    assert!(result.is_none());
}

#[test]
fn empty_batch_returns_zero() {
    let (_conn, repo) = setup();
    assert_eq!(repo.insert_nodes_batch(&[]).unwrap(), 0);
    assert_eq!(repo.insert_edges_batch(&[]).unwrap(), 0);
}

#[test]
fn project_file_tree_uses_source_rows_for_nodes_and_contains_edges() {
    let (conn, repo) = setup();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES ('ds-1', 'case-1', 'source', 'raw', 'source.img')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries
            (id, parent_id, data_source_id, path, name, entry_type)
         VALUES
            ('root', NULL, 'ds-1', '', 'Partition 1', 'directory'),
            ('child', 'root', 'ds-1', 'etc/passwd', 'passwd', 'file')",
        [],
    )
    .unwrap();

    let counts = repo
        .project_file_tree("ds-1", "2026-07-16T00:00:00Z")
        .unwrap();
    assert_eq!(counts, (2, 1));

    let child = repo.get_node("child").unwrap().unwrap();
    assert_eq!(child.case_id, "case-1");
    assert_eq!(child.node_type, NodeType::File);
    assert_eq!(child.label, "passwd");
    assert_eq!(child.summary, "etc/passwd");

    let neighbors = repo
        .get_neighbors("root", &[EdgeType::Contains], Direction::Outgoing)
        .unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0.id, "contains:root:child");
    assert_eq!(neighbors[0].1.id, "child");
}
