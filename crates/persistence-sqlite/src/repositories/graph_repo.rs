use crate::connection::DbResult;
use crate::sql_builder::placeholders;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet, VecDeque};

mod file_projection;

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

#[derive(Debug)]
pub struct GraphNeighborPage {
    pub neighbors: Vec<(GraphEdge, GraphNode)>,
    pub truncated: bool,
}

/// Stable continuation key for graph nodes ordered by creation time and id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNodePageCursor {
    created_at: String,
    id: String,
}

impl From<&GraphNode> for GraphNodePageCursor {
    fn from(node: &GraphNode) -> Self {
        Self {
            created_at: node.created_at.clone(),
            id: node.id.clone(),
        }
    }
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
const GRAPH_NODE_PAGE_BATCH_SIZE: u32 = 256;

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

    pub(crate) fn insert_nodes_batch_unchecked(&self, nodes: &[GraphNode]) -> DbResult<()> {
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

    pub(crate) fn insert_edges_batch_unchecked(&self, edges: &[GraphEdge]) -> DbResult<()> {
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

    /// List graph nodes for a case, newest first. This supports UI citation pickers
    /// without requiring a known seed node.
    pub fn list_nodes_for_case(
        &self,
        case_id: &str,
        limit: u32,
        offset: u32,
    ) -> DbResult<Vec<GraphNode>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut cursor = None;
        let mut remaining = offset;
        while remaining > 0 {
            let batch_size = remaining.min(GRAPH_NODE_PAGE_BATCH_SIZE);
            let batch = self.list_nodes_for_case_after(case_id, batch_size, cursor.as_ref())?;
            if batch.len() < batch_size as usize {
                return Ok(Vec::new());
            }
            cursor = batch.last().map(GraphNodePageCursor::from);
            remaining -= batch_size;
        }

        self.list_nodes_for_case_after(case_id, limit, cursor.as_ref())
    }

    /// Continue listing graph nodes after a stable `(created_at, id)` key.
    pub fn list_nodes_for_case_after(
        &self,
        case_id: &str,
        limit: u32,
        after: Option<&GraphNodePageCursor>,
    ) -> DbResult<Vec<GraphNode>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let (sql, values) = match after {
            Some(cursor) => (
                list_nodes_after_sql(),
                vec![
                    rusqlite::types::Value::Text(case_id.to_string()),
                    rusqlite::types::Value::Text(cursor.created_at.clone()),
                    rusqlite::types::Value::Text(cursor.id.clone()),
                    rusqlite::types::Value::Integer(i64::from(limit)),
                ],
            ),
            None => (
                list_nodes_first_sql(),
                vec![
                    rusqlite::types::Value::Text(case_id.to_string()),
                    rusqlite::types::Value::Integer(i64::from(limit)),
                ],
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), row_to_node)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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

    pub fn get_neighbors_bounded(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
        direction: Direction,
        confidence_floor: Option<f64>,
        limit: u32,
    ) -> DbResult<GraphNeighborPage> {
        if limit == 0 {
            return Ok(GraphNeighborPage {
                neighbors: Vec::new(),
                truncated: false,
            });
        }
        let fetch_limit = limit.saturating_add(1);
        let mut neighbors = match direction {
            Direction::Outgoing => self.query_neighbors_direction(
                node_id,
                edge_types,
                confidence_floor,
                fetch_limit,
                true,
            )?,
            Direction::Incoming => self.query_neighbors_direction(
                node_id,
                edge_types,
                confidence_floor,
                fetch_limit,
                false,
            )?,
            Direction::Both => {
                let mut rows = self.query_neighbors_direction(
                    node_id,
                    edge_types,
                    confidence_floor,
                    fetch_limit,
                    true,
                )?;
                rows.extend(self.query_neighbors_direction(
                    node_id,
                    edge_types,
                    confidence_floor,
                    fetch_limit,
                    false,
                )?);
                rows.sort_by(|left, right| left.0.id.cmp(&right.0.id));
                rows.dedup_by(|left, right| left.0.id == right.0.id);
                rows
            }
        };
        let truncated = neighbors.len() > limit as usize;
        neighbors.truncate(limit as usize);
        Ok(GraphNeighborPage {
            neighbors,
            truncated,
        })
    }

    fn query_neighbors_direction(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
        confidence_floor: Option<f64>,
        limit: u32,
        outgoing: bool,
    ) -> DbResult<Vec<(GraphEdge, GraphNode)>> {
        let (match_column, neighbor_column) = if outgoing {
            ("e.source_id", "e.target_id")
        } else {
            ("e.target_id", "e.source_id")
        };
        let mut values = vec![rusqlite::types::Value::Text(node_id.to_string())];
        let mut filters = Vec::new();
        if !edge_types.is_empty() {
            let first = values.len() + 1;
            filters.push(format!(
                "e.edge_type IN ({})",
                placeholders(first, edge_types.len())
            ));
            values.extend(
                edge_types
                    .iter()
                    .map(|edge_type| rusqlite::types::Value::Text(edge_type_str(edge_type).into())),
            );
        }
        if let Some(floor) = confidence_floor {
            values.push(rusqlite::types::Value::Real(floor));
            filters.push(format!("COALESCE(e.confidence, 0.0) >= ?{}", values.len()));
        }
        values.push(rusqlite::types::Value::Integer(i64::from(limit)));
        let extra_filter = if filters.is_empty() {
            String::new()
        } else {
            format!(" AND {}", filters.join(" AND "))
        };
        let sql = format!(
            "SELECT e.id, e.case_id, e.source_id, e.target_id, e.edge_type,
                    e.confidence, e.provenance, e.created_at,
                    n.id, n.case_id, n.node_type, n.label, n.summary, n.tags,
                    n.created_at
             FROM graph_edges e
             JOIN graph_nodes n ON n.id = {neighbor_column}
             WHERE {match_column} = ?1{extra_filter}
             ORDER BY e.id ASC
             LIMIT ?{}",
            values.len()
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows =
            statement.query_map(rusqlite::params_from_iter(values), row_to_edge_node_pair)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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

    /// Find a graph edge by its id.
    pub fn find_edge_by_id(&self, edge_id: &str) -> DbResult<Option<GraphEdge>> {
        let sql = format!("SELECT {EDGE_COLUMNS} FROM graph_edges WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let result = stmt.query_row(params![edge_id], row_to_edge);
        match result {
            Ok(edge) => Ok(Some(edge)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find edges with provenance metadata for a case (used for incremental rule pack execution).
    pub fn find_edges_with_provenance_by_case(
        &self,
        case_id: &str,
        edge_type: &str,
    ) -> DbResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT provenance FROM graph_edges
             WHERE case_id = ?1 AND edge_type = ?2
             AND provenance IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![case_id, edge_type], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find graph nodes by type for a case.
    pub fn find_nodes_by_type_for_case(
        &self,
        case_id: &str,
        node_type: &str,
    ) -> DbResult<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, summary FROM graph_nodes WHERE case_id = ?1 AND node_type = ?2",
        )?;
        let rows = stmt.query_map(params![case_id, node_type], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row?);
        }
        Ok(nodes)
    }

    pub fn list_nodes_by_type_for_case_bounded(
        &self,
        case_id: &str,
        node_type: &NodeType,
        limit: u32,
    ) -> DbResult<Vec<GraphNode>> {
        let sql = format!(
            "SELECT {NODE_COLUMNS} FROM graph_nodes
             WHERE case_id = ?1 AND node_type = ?2
             ORDER BY id ASC
             LIMIT ?3"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![case_id, node_type_str(node_type), limit],
            row_to_node,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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

fn list_nodes_first_sql() -> String {
    format!(
        "SELECT {NODE_COLUMNS} FROM graph_nodes
         WHERE case_id = ?1
         ORDER BY created_at DESC, id ASC
         LIMIT ?2"
    )
}

fn list_nodes_after_sql() -> String {
    format!(
        "SELECT {NODE_COLUMNS} FROM graph_nodes
         WHERE case_id = ?1
           AND (created_at < ?2 OR (created_at = ?2 AND id > ?3))
         ORDER BY created_at DESC, id ASC
         LIMIT ?4"
    )
}

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

    params.extend(edge_types.iter().map(|et| edge_type_str(et).to_string()));

    let sql = format!(
        "{} AND edge_type IN ({})",
        base_sql,
        placeholders(start_param + 1, edge_types.len())
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
#[path = "../../tests/unit/graph_repo.rs"]
mod tests;
