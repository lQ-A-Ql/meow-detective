use crate::connection::DbResult;
use domain::{GraphNode, NodeType};
use rusqlite::params;

use super::{
    mapping::{node_type_str, row_to_node, NODE_COLUMNS},
    GraphNodePageCursor, GraphRepo,
};

const GRAPH_NODE_PAGE_BATCH_SIZE: u32 = 256;

impl GraphRepo<'_> {
    /// Retrieve a single graph node by id.
    pub fn get_node(&self, node_id: &str) -> DbResult<Option<GraphNode>> {
        let sql = format!("SELECT {NODE_COLUMNS} FROM graph_nodes WHERE id = ?1");
        let mut statement = self.conn.prepare(&sql)?;
        match statement.query_row(params![node_id], row_to_node) {
            Ok(node) => Ok(Some(node)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// List graph nodes for a case, newest first.
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
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(values), row_to_node)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Find graph nodes by type for a case.
    pub fn find_nodes_by_type_for_case(
        &self,
        case_id: &str,
        node_type: &str,
    ) -> DbResult<Vec<(String, String, String)>> {
        let mut statement = self.conn.prepare(
            "SELECT id, label, summary FROM graph_nodes WHERE case_id = ?1 AND node_type = ?2",
        )?;
        let rows = statement.query_map(params![case_id, node_type], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
}

fn list_nodes_first_sql() -> String {
    format!(
        "SELECT {NODE_COLUMNS} FROM graph_nodes
         WHERE case_id = ?1
         ORDER BY created_at DESC, id ASC
         LIMIT ?2"
    )
}

pub(super) fn list_nodes_after_sql() -> String {
    format!(
        "SELECT {NODE_COLUMNS} FROM graph_nodes
         WHERE case_id = ?1
           AND (created_at < ?2 OR (created_at = ?2 AND id > ?3))
         ORDER BY created_at DESC, id ASC
         LIMIT ?4"
    )
}
