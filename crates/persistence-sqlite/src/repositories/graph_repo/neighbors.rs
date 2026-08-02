use crate::{connection::DbResult, sql_builder::placeholders};
use domain::{EdgeType, GraphEdge, GraphNode};

use super::{
    mapping::{edge_type_str, row_to_edge_node_pair},
    Direction, GraphNeighborPage, GraphRepo,
};

impl GraphRepo<'_> {
    /// Retrieve neighbors of a node, optionally filtered by edge types and direction.
    pub fn get_neighbors(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
        direction: Direction,
    ) -> DbResult<Vec<(GraphEdge, GraphNode)>> {
        let (source_column, target_column) = match direction {
            Direction::Outgoing => ("e.source_id", "e.target_id"),
            Direction::Incoming => ("e.target_id", "e.source_id"),
            Direction::Both => return self.get_neighbors_both(node_id, edge_types),
        };

        let (sql, parameters) = build_neighbor_query(
            &format!(
                "SELECT e.id, e.case_id, e.source_id, e.target_id, e.edge_type, e.confidence, e.provenance, e.created_at,
                        n.id, n.case_id, n.node_type, n.label, n.summary, n.tags, n.created_at
                 FROM graph_edges e
                 JOIN graph_nodes n ON n.id = {target_column}
                 WHERE {source_column} = ?1"
            ),
            node_id,
            edge_types,
            1,
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(parameters.iter()),
            row_to_edge_node_pair,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
        let (sql, parameters) = build_neighbor_query(
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
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(parameters.iter()),
            row_to_edge_node_pair,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub(super) fn build_neighbor_query(
    base_sql: &str,
    node_id: &str,
    edge_types: &[EdgeType],
    start_parameter: usize,
) -> (String, Vec<String>) {
    let mut parameters = vec![node_id.to_string()];
    if edge_types.is_empty() {
        return (base_sql.to_string(), parameters);
    }
    parameters.extend(
        edge_types
            .iter()
            .map(|edge_type| edge_type_str(edge_type).to_string()),
    );
    let sql = format!(
        "{} AND edge_type IN ({})",
        base_sql,
        placeholders(start_parameter + 1, edge_types.len())
    );
    (sql, parameters)
}
