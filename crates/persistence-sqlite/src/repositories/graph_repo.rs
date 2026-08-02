use std::collections::HashMap;

use domain::{GraphEdge, GraphNode};
use rusqlite::Connection;

mod edges;
mod file_projection;
mod mapping;
mod neighbors;
mod nodes;
mod snapshot;
mod traversal;
mod writes;

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

pub struct GraphRepo<'a> {
    pub(super) conn: &'a Connection,
}

impl<'a> GraphRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/graph_repo.rs"]
mod tests;
