use std::collections::HashMap;

use crate::connection::DbResult;
use rusqlite::params;

use super::{GraphRepo, GraphSnapshot};

impl GraphRepo<'_> {
    /// Compute aggregate graph statistics for a case.
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
        Ok(GraphSnapshot {
            node_count_by_type: self.count_by_column(
                "graph_nodes",
                "node_type",
                "case_id",
                case_id,
            )?,
            edge_count_by_type: self.count_by_column(
                "graph_edges",
                "edge_type",
                "case_id",
                case_id,
            )?,
            total_nodes: total_nodes as u64,
            total_edges: total_edges as u64,
        })
    }

    fn count_by_column(
        &self,
        table: &str,
        group_column: &str,
        filter_column: &str,
        filter_value: &str,
    ) -> DbResult<HashMap<String, u64>> {
        let sql = format!(
            "SELECT {group_column}, COUNT(*) FROM {table} WHERE {filter_column} = ?1 GROUP BY {group_column}"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![filter_value], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = HashMap::new();
        for row in rows {
            let (key, count) = row?;
            counts.insert(key, count as u64);
        }
        Ok(counts)
    }
}
