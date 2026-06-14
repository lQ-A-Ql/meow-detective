use crate::connection::DbResult;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet, VecDeque};

/// Direction for neighbor traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

/// Aggregate statistics snapshot of the investigative graph for a case.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphSnapshot {
    pub node_count_by_type: HashMap<String, u64>,
    pub edge_count_by_type: HashMap<String, u64>,
    pub total_nodes: u64,
    pub total_edges: u64,
}

const INSERT_NODES_SQL: &str =
    "INSERT OR REPLACE INTO graph_nodes (id, case_id, node_type, label, summary, tags, created_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

const INSERT_EDGES_SQL: &str =
    "INSERT OR REPLACE INTO graph_edges (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

const NODE_COLUMNS: &str = "id, case_id, node_type, label, summary, tags, created_at";
const EDGE_COLUMNS: &str =
    "id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at";

pub struct GraphRepo<'a> {
    conn: &'a Connection,
}

impl<'a> GraphRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert multiple graph nodes in a single transaction, returning the count inserted.
    pub fn insert_nodes_batch(&self, nodes: &[GraphNode]) -> DbResult<u64> {
        if nodes.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let repo = GraphRepo::new(&tx);
            repo.insert_nodes_batch_unchecked(nodes)?;
        }
        tx.commit()?;
        Ok(nodes.len() as u64)
    }

    fn insert_nodes_batch_unchecked(&self, nodes: &[GraphNode]) -> DbResult<()> {
        if nodes.is_empty() {
            return Ok(());
        }
        let mut stmt = self.conn.prepare_cached(INSERT_NODES_SQL)?;
        for node in nodes {
            let tags_json = serde_json::to_string(&node.tags).unwrap_or_default();
            stmt.execute(params![
                node.id,
                node.case_id,
                node_type_str(&node.node_type),
                node.label,
                node.summary,
                tags_json,
                node.created_at,
            ])?;
        }
        Ok(())
    }

    /// Insert multiple graph edges in a single transaction, returning the count inserted.
    pub fn insert_edges_batch(&self, edges: &[GraphEdge]) -> DbResult<u64> {
        if edges.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let repo = GraphRepo::new(&tx);
            repo.insert_edges_batch_unchecked(edges)?;
        }
        tx.commit()?;
        Ok(edges.len() as u64)
    }

    fn insert_edges_batch_unchecked(&self, edges: &[GraphEdge]) -> DbResult<()> {
        if edges.is_empty() {
            return Ok(());
        }
        let mut stmt = self.conn.prepare_cached(INSERT_EDGES_SQL)?;
        for edge in edges {
            stmt.execute(params![
                edge.id,
                edge.case_id,
                edge.source_id,
                edge.target_id,
                edge_type_str(&edge.edge_type),
                edge.confidence,
                edge.provenance,
                edge.created_at,
            ])?;
        }
        Ok(())
    }

    /// Retrieve a single graph node by id.
    pub fn get_node(&self, node_id: &str) -> DbResult<Option<GraphNode>> {
        let sql = format!("SELECT {NODE_COLUMNS} FROM graph_nodes WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let result = stmt.query_row(params![node_id], row_to_node);
        match result {
            Ok(node) => Ok(Some(node)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Retrieve neighbors of a node, optionally filtered by edge types and direction.
    /// Returns edges paired with the neighbor node at the other end.
    pub fn get_neighbors(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
        direction: Direction,
    ) -> DbResult<Vec<(GraphEdge, GraphNode)>> {
        let (source_col, target_col) = match direction {
            Direction::Outgoing => ("e.source_id", "e.target_id"),
            Direction::Incoming => ("e.target_id", "e.source_id"),
            Direction::Both => {
                return self.get_neighbors_both(node_id, edge_types);
            }
        };

        let (sql, params) = build_neighbor_query(
            &format!(
                "SELECT e.id, e.case_id, e.source_id, e.target_id, e.edge_type, e.confidence, e.provenance, e.created_at,
                        n.id, n.case_id, n.node_type, n.label, n.summary, n.tags, n.created_at
                 FROM graph_edges e
                 JOIN graph_nodes n ON n.id = {target_col}
                 WHERE {source_col} = ?1"
            ),
            node_id,
            edge_types,
            1,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter()),
            row_to_edge_node_pair,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    fn get_neighbors_both(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
    ) -> DbResult<Vec<(GraphEdge, GraphNode)>> {
        let (sql, params) = build_neighbor_query(
            "SELECT e.id, e.case_id, e.source_id, e.target_id, e.edge_type, e.confidence, e.provenance, e.created_at,
                    n.id, n.case_id, n.node_type, n.label, n.summary, n.tags, n.created_at
             FROM graph_edges e
             JOIN graph_nodes n ON n.id = e.target_id
             WHERE e.source_id = ?1
             UNION
             SELECT e.id, e.case_id, e.source_id, e.target_id, e.edge_type, e.confidence, e.provenance, e.created_at,
                    n.id, n.case_id, n.node_type, n.label, n.summary, n.tags, n.created_at
             FROM graph_edges e
             JOIN graph_nodes n ON n.id = e.source_id
             WHERE e.target_id = ?1",
            node_id,
            edge_types,
            1,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter()),
            row_to_edge_node_pair,
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// BFS traversal of the graph from start_ids, following edges of the given types,
    /// up to max_depth hops, returning at most limit nodes and their connecting edges.
    pub fn traverse(
        &self,
        start_ids: &[String],
        edge_types: &[EdgeType],
        max_depth: u32,
        limit: u32,
    ) -> DbResult<(Vec<GraphNode>, Vec<GraphEdge>)> {
        if start_ids.is_empty() || max_depth == 0 || limit == 0 {
            return Ok((Vec::new(), Vec::new()));
        }

        let edge_type_set: HashSet<String> = if edge_types.is_empty() {
            HashSet::new()
        } else {
            edge_types
                .iter()
                .map(|et| edge_type_str(et).to_string())
                .collect()
        };

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_edges: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();
        // Collect nodes and edges as we discover them
        let mut result_nodes: Vec<GraphNode> = Vec::new();
        let mut result_edges: Vec<GraphEdge> = Vec::new();

        // Seed the BFS queue with start nodes
        for start_id in start_ids {
            if let Some(node) = self.get_node(start_id)? {
                if visited_nodes.insert(node.id.clone()) {
                    result_nodes.push(node);
                    queue.push_back((start_id.clone(), 0));
                }
            }
        }

        // BFS
        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if result_nodes.len() >= limit as usize {
                break;
            }

            // Fetch all neighbors (outgoing edges) from current node
            let neighbors = self.get_neighbors_raw(&current_id, &edge_type_set)?;

            for (edge, neighbor_id) in neighbors {
                // Check limit
                if result_nodes.len() >= limit as usize {
                    break;
                }

                // Record edge if not seen
                if visited_edges.insert(edge.id.clone()) {
                    result_edges.push(edge);
                }

                // Record neighbor node if not seen
                if !visited_nodes.contains(&neighbor_id) {
                    if let Some(node) = self.get_node(&neighbor_id)? {
                        visited_nodes.insert(neighbor_id.clone());
                        result_nodes.push(node);
                        queue.push_back((neighbor_id, depth + 1));
                    }
                }
            }
        }

        Ok((result_nodes, result_edges))
    }

    /// Internal: fetch all outgoing edges from a node with optional edge type filter.
    /// Returns (edge, target_node_id) pairs.
    fn get_neighbors_raw(
        &self,
        node_id: &str,
        edge_type_set: &HashSet<String>,
    ) -> DbResult<Vec<(GraphEdge, String)>> {
        let base_query = format!("SELECT {EDGE_COLUMNS} FROM graph_edges WHERE source_id = ?1");

        let edge_types: Vec<EdgeType> = edge_type_set.iter().map(|s| parse_edge_type(s)).collect();

        let (sql, params) = build_neighbor_query(&base_query, node_id, &edge_types, 1);

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row_to_edge(row)?, row.get::<_, String>(3)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Compute aggregate snapshot statistics for the graph in a given case.
    pub fn get_snapshot(&self, case_id: &str) -> DbResult<GraphSnapshot> {
        let total_nodes: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE case_id = ?1",
            params![case_id],
            |row| row.get(0),
        )?;

        let total_edges: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM graph_edges WHERE case_id = ?1",
            params![case_id],
            |row| row.get(0),
        )?;

        let node_counts = self.count_by_column("graph_nodes", "node_type", "case_id", case_id)?;

        let edge_counts = self.count_by_column("graph_edges", "edge_type", "case_id", case_id)?;

        Ok(GraphSnapshot {
            node_count_by_type: node_counts,
            edge_count_by_type: edge_counts,
            total_nodes: total_nodes as u64,
            total_edges: total_edges as u64,
        })
    }

    /// Delete all graph nodes and edges for a given case.
    pub fn delete_case_graph(&self, case_id: &str) -> DbResult<()> {
        // The foreign key with ON DELETE CASCADE from edges to nodes means deleting
        // nodes will cascade to edges. But we also need to handle the case where
        // edges reference nodes from other cases (shouldn't happen, but be safe).
        self.conn.execute(
            "DELETE FROM graph_edges WHERE case_id = ?1",
            params![case_id],
        )?;
        self.conn.execute(
            "DELETE FROM graph_nodes WHERE case_id = ?1",
            params![case_id],
        )?;
        Ok(())
    }

    /// Helper: count rows grouped by a column with a filter.
    fn count_by_column(
        &self,
        table: &str,
        group_col: &str,
        filter_col: &str,
        filter_val: &str,
    ) -> DbResult<HashMap<String, u64>> {
        let sql = format!(
            "SELECT {group_col}, COUNT(*) FROM {table} WHERE {filter_col} = ?1 GROUP BY {group_col}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![filter_val], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut map = HashMap::new();
        for row in rows {
            let (key, count) = row?;
            map.insert(key, count as u64);
        }
        Ok(map)
    }
}

// ── Serialization helpers ──

fn node_type_str(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

fn parse_node_type(s: &str) -> NodeType {
    match s {
        "file" => NodeType::File,
        "artifact" => NodeType::Artifact,
        "timeline_event" => NodeType::TimelineEvent,
        "entity" => NodeType::Entity,
        "lead" => NodeType::Lead,
        "notebook_entry" => NodeType::NotebookEntry,
        _ => NodeType::Entity, // conservative fallback
    }
}

fn edge_type_str(et: &EdgeType) -> &'static str {
    match et {
        EdgeType::Contains => "contains",
        EdgeType::References => "references",
        EdgeType::CorrelatesWith => "correlates_with",
        EdgeType::DerivesFrom => "derives_from",
        EdgeType::Precedes => "precedes",
        EdgeType::Cites => "cites",
        EdgeType::Annotates => "annotates",
    }
}

fn parse_edge_type(s: &str) -> EdgeType {
    match s {
        "contains" => EdgeType::Contains,
        "references" => EdgeType::References,
        "correlates_with" => EdgeType::CorrelatesWith,
        "derives_from" => EdgeType::DerivesFrom,
        "precedes" => EdgeType::Precedes,
        "cites" => EdgeType::Cites,
        "annotates" => EdgeType::Annotates,
        _ => EdgeType::References, // conservative fallback
    }
}

/// Build a SQL query with optional edge type IN filter, and a params Vec.
/// `base_sql` must contain a `WHERE ... ?1` clause that consumes the first param.
/// Returns (sql_with_filter, params_vec) where params_vec has node_id first, then edge_type strings.
fn build_neighbor_query(
    base_sql: &str,
    node_id: &str,
    edge_types: &[EdgeType],
    start_param: usize,
) -> (String, Vec<String>) {
    let mut params: Vec<String> = vec![node_id.to_string()];

    if edge_types.is_empty() {
        return (base_sql.to_string(), params);
    }

    let placeholders: Vec<String> = edge_types
        .iter()
        .enumerate()
        .map(|(i, et)| {
            params.push(edge_type_str(et).to_string());
            format!("?{}", start_param + 1 + i)
        })
        .collect();

    let sql = format!(
        "{} AND edge_type IN ({})",
        base_sql,
        placeholders.join(", ")
    );
    (sql, params)
}

// ── Row mappers ──

fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<GraphNode> {
    let tags_str: String = row.get(5)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(GraphNode {
        id: row.get(0)?,
        case_id: row.get(1)?,
        node_type: parse_node_type(&row.get::<_, String>(2)?),
        label: row.get(3)?,
        summary: row.get(4)?,
        tags,
        created_at: row.get(6)?,
    })
}

fn row_to_edge(row: &rusqlite::Row) -> rusqlite::Result<GraphEdge> {
    Ok(GraphEdge {
        id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        target_id: row.get(3)?,
        edge_type: parse_edge_type(&row.get::<_, String>(4)?),
        confidence: row.get(5)?,
        provenance: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn row_to_edge_node_pair(row: &rusqlite::Row) -> rusqlite::Result<(GraphEdge, GraphNode)> {
    let edge = GraphEdge {
        id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        target_id: row.get(3)?,
        edge_type: parse_edge_type(&row.get::<_, String>(4)?),
        confidence: row.get(5)?,
        provenance: row.get(6)?,
        created_at: row.get(7)?,
    };
    let node = GraphNode {
        id: row.get(8)?,
        case_id: row.get(9)?,
        node_type: parse_node_type(&row.get::<_, String>(10)?),
        label: row.get(11)?,
        summary: row.get(12)?,
        tags: serde_json::from_str(&row.get::<_, String>(13)?).unwrap_or_default(),
        created_at: row.get(14)?,
    };
    Ok((edge, node))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connection::open_in_memory, runner};

    fn setup() -> (&'static Connection, GraphRepo<'static>) {
        // open_in_memory returns a Connection we need to own, but GraphRepo borrows it.
        // We use a leaked Box to get a 'static reference for testing convenience.
        let conn = Box::new(open_in_memory().unwrap());
        let conn_ref: &'static Connection = Box::leak(conn);
        runner::run_all(conn_ref).unwrap();
        // Insert a dummy case for foreign key
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
            summary: format!("Summary for {}", id),
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
        let count = repo.insert_nodes_batch(&[node.clone()]).unwrap();
        assert_eq!(count, 1);

        let fetched = repo.get_node("n1").unwrap().expect("node should exist");
        assert_eq!(fetched.id, "n1");
        assert_eq!(fetched.node_type, NodeType::File);
        assert_eq!(fetched.label, "cmd.exe");
        assert_eq!(fetched.tags, vec!["test"]);
    }

    #[test]
    fn insert_and_get_edge() {
        let (_conn, repo) = setup();
        let n1 = make_node("n1", NodeType::File, "a.exe");
        let n2 = make_node("n2", NodeType::Artifact, "LNK");
        repo.insert_nodes_batch(&[n1, n2]).unwrap();

        let edge = make_edge("e1", "n1", "n2", EdgeType::References);
        let count = repo.insert_edges_batch(&[edge.clone()]).unwrap();
        assert_eq!(count, 1);

        // Verify edge exists via neighbor query
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
        // n1 -> n2 (outgoing) and n3 -> n1 (incoming)
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
        // Create a path: n1 -> n2 -> n3 -> n4
        let nodes: Vec<GraphNode> = (1..=4)
            .map(|i| make_node(&format!("n{}", i), NodeType::File, &format!("node{}", i)))
            .collect();
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
        let nodes: Vec<GraphNode> = (1..=5)
            .map(|i| make_node(&format!("n{}", i), NodeType::File, &format!("node{}", i)))
            .collect();
        repo.insert_nodes_batch(&nodes).unwrap();
        repo.insert_edges_batch(&[
            make_edge("e1", "n1", "n2", EdgeType::References),
            make_edge("e2", "n2", "n3", EdgeType::References),
            make_edge("e3", "n3", "n4", EdgeType::References),
            make_edge("e4", "n4", "n5", EdgeType::References),
        ])
        .unwrap();

        let (result_nodes, _) = repo.traverse(&["n1".to_string()], &[], 1, 100).unwrap();
        // depth 1: n1 + n2 = 2 nodes
        assert_eq!(result_nodes.len(), 2);
    }

    #[test]
    fn traverse_respects_limit() {
        let (_conn, repo) = setup();
        let nodes: Vec<GraphNode> = (1..=5)
            .map(|i| make_node(&format!("n{}", i), NodeType::File, &format!("node{}", i)))
            .collect();
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
}
