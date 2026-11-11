use super::GraphRepo;
use crate::connection::DbResult;

/// One source-detail file record projected into the case graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeProjectionRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub path: String,
}

/// One page of source-detail file records plus the cursor for the next page.
///
/// Cursors are opaque key bytes produced by the source-detail store; the graph
/// crate only threads them back through `fetch_page`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeProjectionPage {
    pub records: Vec<FileTreeProjectionRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

impl GraphRepo<'_> {
    /// Project a source-local file tree into graph nodes and containment edges.
    ///
    /// The projection stays inside a SQLite transaction while the caller
    /// supplies paged source-detail records through `fetch_page`, so large trees
    /// do not require OFFSET scans, Rust-side row materialization, or per-chunk
    /// commits here.
    pub fn project_file_tree<F>(
        &self,
        data_source_id: &str,
        created_at: &str,
        mut fetch_page: F,
    ) -> DbResult<(u64, u64)>
    where
        F: FnMut(Option<&[u8]>) -> DbResult<FileTreeProjectionPage>,
    {
        use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
        let case: String = self.conn.query_row(
            "SELECT case_id FROM data_sources WHERE id = ?1",
            [data_source_id],
            |row| row.get(0),
        )?;
        let tx = self.conn.unchecked_transaction()?;
        let graph = GraphRepo::new(&tx);
        let mut cursor: Option<Vec<u8>> = None;
        let mut node_count = 0;
        let mut edge_count = 0;
        loop {
            crate::read_operation::ensure_active()?;
            let page = fetch_page(cursor.as_deref())?;
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
                        created_at: created_at.into(),
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
                        created_at: created_at.into(),
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
