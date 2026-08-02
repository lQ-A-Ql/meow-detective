use std::collections::{HashSet, VecDeque};

use crate::connection::DbResult;
use domain::{EdgeType, GraphEdge, GraphNode};

use super::{
    mapping::{edge_type_str, parse_edge_type, row_to_edge, EDGE_COLUMNS},
    neighbors::build_neighbor_query,
    GraphRepo,
};

impl GraphRepo<'_> {
    /// Breadth-first traversal from `start_ids` through outgoing edges.
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

        let edge_type_set = edge_types
            .iter()
            .map(|edge_type| edge_type_str(edge_type).to_string())
            .collect::<HashSet<_>>();
        let mut visited_nodes = HashSet::new();
        let mut visited_edges = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result_nodes = Vec::new();
        let mut result_edges = Vec::new();

        for start_id in start_ids {
            if let Some(node) = self.get_node(start_id)? {
                if visited_nodes.insert(node.id.clone()) {
                    result_nodes.push(node);
                    queue.push_back((start_id.clone(), 0));
                }
            }
        }

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if result_nodes.len() >= limit as usize {
                break;
            }
            for (edge, neighbor_id) in self.get_neighbors_raw(&current_id, &edge_type_set)? {
                if result_nodes.len() >= limit as usize {
                    break;
                }
                if visited_edges.insert(edge.id.clone()) {
                    result_edges.push(edge);
                }
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

    fn get_neighbors_raw(
        &self,
        node_id: &str,
        edge_type_set: &HashSet<String>,
    ) -> DbResult<Vec<(GraphEdge, String)>> {
        let base_query = format!("SELECT {EDGE_COLUMNS} FROM graph_edges WHERE source_id = ?1");
        let edge_types = edge_type_set
            .iter()
            .map(|value| parse_edge_type(value))
            .collect::<Vec<_>>();
        let (sql, parameters) = build_neighbor_query(&base_query, node_id, &edge_types, 1);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters.iter()), |row| {
            Ok((row_to_edge(row)?, row.get(3)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
