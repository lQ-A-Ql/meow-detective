use crate::connection::DbResult;
use domain::{GraphEdge, GraphNode};
use rusqlite::params;

use super::{mapping::edge_type_str, mapping::node_type_str, GraphRepo};

const INSERT_NODES_SQL: &str =
    "INSERT OR REPLACE INTO graph_nodes (id, case_id, node_type, label, summary, tags, created_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
const INSERT_EDGES_SQL: &str =
    "INSERT OR REPLACE INTO graph_edges (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

impl GraphRepo<'_> {
    /// Insert multiple graph nodes in a single transaction, returning the count inserted.
    pub fn insert_nodes_batch(&self, nodes: &[GraphNode]) -> DbResult<u64> {
        if nodes.is_empty() {
            return Ok(0);
        }
        let transaction = self.conn.unchecked_transaction()?;
        GraphRepo::new(&transaction).insert_nodes_batch_unchecked(nodes)?;
        transaction.commit()?;
        Ok(nodes.len() as u64)
    }

    pub(crate) fn insert_nodes_batch_unchecked(&self, nodes: &[GraphNode]) -> DbResult<()> {
        if nodes.is_empty() {
            return Ok(());
        }
        let mut statement = self.conn.prepare_cached(INSERT_NODES_SQL)?;
        for node in nodes {
            let tags_json = serde_json::to_string(&node.tags).unwrap_or_default();
            statement.execute(params![
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
        let transaction = self.conn.unchecked_transaction()?;
        GraphRepo::new(&transaction).insert_edges_batch_unchecked(edges)?;
        transaction.commit()?;
        Ok(edges.len() as u64)
    }

    pub(crate) fn insert_edges_batch_unchecked(&self, edges: &[GraphEdge]) -> DbResult<()> {
        if edges.is_empty() {
            return Ok(());
        }
        let mut statement = self.conn.prepare_cached(INSERT_EDGES_SQL)?;
        for edge in edges {
            statement.execute(params![
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

    /// Delete all graph nodes and edges for a given case.
    pub fn delete_case_graph(&self, case_id: &str) -> DbResult<()> {
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
}
