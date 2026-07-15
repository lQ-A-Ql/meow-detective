use rusqlite::params;

use super::GraphRepo;
use crate::connection::DbResult;

impl GraphRepo<'_> {
    /// Project a source-local file tree into graph nodes and containment edges.
    ///
    /// The projection stays inside SQLite so large trees do not require
    /// OFFSET scans, Rust-side row materialization, or per-chunk commits.
    pub fn project_file_tree(
        &self,
        data_source_id: &str,
        created_at: &str,
    ) -> DbResult<(u64, u64)> {
        let tx = self.conn.unchecked_transaction()?;
        let case_id: String = tx.query_row(
            "SELECT case_id FROM data_sources WHERE id = ?1",
            params![data_source_id],
            |row| row.get(0),
        )?;

        let node_count = tx.execute(
            "INSERT OR REPLACE INTO graph_nodes
                (id, case_id, node_type, label, summary, tags, created_at)
             SELECT id, ?2, 'file', name, path, '[]', ?3
             FROM file_entries
             WHERE data_source_id = ?1
               AND NOT (name = '' AND parent_id IS NULL)",
            params![data_source_id, case_id, created_at],
        )?;
        let edge_count = tx.execute(
            "INSERT OR REPLACE INTO graph_edges
                (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
             SELECT 'contains:' || parent_id || ':' || id,
                    ?2, parent_id, id, 'contains', NULL, NULL, ?3
             FROM file_entries
             WHERE data_source_id = ?1
               AND parent_id IS NOT NULL",
            params![data_source_id, case_id, created_at],
        )?;

        tx.commit()?;
        Ok((node_count as u64, edge_count as u64))
    }
}
