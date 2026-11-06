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
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            return self.project_fjall_file_tree(&store, data_source_id, created_at);
        }
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

impl GraphRepo<'_> {
    fn project_fjall_file_tree(
        &self,
        store: &persistence_fjall::SourceStore,
        source: &str,
        created: &str,
    ) -> DbResult<(u64, u64)> {
        use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
        let case: String = self.conn.query_row(
            "SELECT case_id FROM data_sources WHERE id = ?1",
            [source],
            |row| row.get(0),
        )?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store(store.clone());
        let snapshot = catalog.read_snapshot();
        let tx = self.conn.unchecked_transaction()?;
        let graph = GraphRepo::new(&tx);
        let mut cursor = None;
        let mut node_count = 0;
        let mut edge_count = 0;
        loop {
            crate::read_operation::ensure_active()?;
            let page = catalog
                .list_all(
                    &snapshot,
                    source,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| crate::DbError::System(error.to_string()))?;
            let mut nodes = Vec::with_capacity(page.records.len());
            let mut edges = Vec::with_capacity(page.records.len());
            for record in page.records {
                if !record.name.is_empty() || record.parent_id.is_some() {
                    nodes.push(GraphNode {
                        id: record.id.clone(),
                        case_id: case.clone(),
                        node_type: NodeType::File,
                        label: record.name,
                        summary: record.path,
                        tags: Vec::new(),
                        created_at: created.into(),
                    });
                }
                if let Some(parent) = record.parent_id {
                    edges.push(GraphEdge {
                        id: format!("contains:{parent}:{}", record.id),
                        case_id: case.clone(),
                        source_id: parent,
                        target_id: record.id,
                        edge_type: EdgeType::Contains,
                        confidence: None,
                        provenance: None,
                        created_at: created.into(),
                    });
                }
            }
            graph.insert_nodes_batch_unchecked(&nodes)?;
            graph.insert_edges_batch_unchecked(&edges)?;
            node_count += nodes.len() as u64;
            edge_count += edges.len() as u64;
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        tx.commit()?;
        Ok((node_count, edge_count))
    }
}
