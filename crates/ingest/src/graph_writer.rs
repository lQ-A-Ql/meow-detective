//! GraphWriter trait and SQLite-backed implementation.
//!
//! The [`GraphWriter`] trait abstracts the write side of an investigative graph,
//! allowing ingest pipelines to emit nodes and edges without coupling to a
//! specific storage backend. The [`SqliteGraphWriter`] provides a concrete
//! implementation backed by [`persistence_sqlite::repositories::GraphRepo`].

use domain::{GraphEdge, GraphNode};
use persistence_sqlite::repositories::graph_repo::GraphRepo;

/// Trait for writing graph nodes and edges into an investigative graph store.
///
/// Implementations may batch writes, buffer internally, or write directly to
/// persistent storage. The returned `u64` is the number of items actually
/// written in the current call.
pub trait GraphWriter {
    /// Write a batch of graph nodes, returning the count written.
    fn write_nodes(&mut self, nodes: &[GraphNode]) -> Result<u64, String>;

    /// Write a batch of graph edges, returning the count written.
    fn write_edges(&mut self, edges: &[GraphEdge]) -> Result<u64, String>;
}

/// SQLite-backed [`GraphWriter`] that delegates to [`GraphRepo`].
///
/// Tracks cumulative `node_count` and `edge_count` for live reporting during
/// long-running ingest pipelines.
pub struct SqliteGraphWriter<'a> {
    repo: GraphRepo<'a>,
    pub node_count: u64,
    pub edge_count: u64,
}

impl<'a> SqliteGraphWriter<'a> {
    /// Create a new writer wrapping the given [`GraphRepo`].
    ///
    /// Initial cumulative counts are zero.
    pub fn new(repo: GraphRepo<'a>) -> Self {
        Self {
            repo,
            node_count: 0,
            edge_count: 0,
        }
    }
}

impl<'a> GraphWriter for SqliteGraphWriter<'a> {
    fn write_nodes(&mut self, nodes: &[GraphNode]) -> Result<u64, String> {
        let count = self
            .repo
            .insert_nodes_batch(nodes)
            .map_err(|e| e.to_string())?;
        self.node_count += count;
        Ok(count)
    }

    fn write_edges(&mut self, edges: &[GraphEdge]) -> Result<u64, String> {
        let count = self
            .repo
            .insert_edges_batch(edges)
            .map_err(|e| e.to_string())?;
        self.edge_count += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
    use persistence_sqlite::repositories::graph_repo::GraphRepo;
    use rusqlite::Connection;

    fn create_graph_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS graph_nodes (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                node_type TEXT NOT NULL,
                label TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS graph_edges (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                confidence REAL,
                provenance TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (source_id) REFERENCES graph_nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (target_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
            );
            PRAGMA foreign_keys = ON;",
        )
        .expect("create graph tables");
    }

    fn make_node(id: &str, label: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            case_id: "test-case".to_string(),
            node_type: NodeType::File,
            label: label.to_string(),
            summary: String::new(),
            tags: vec![],
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_edge(id: &str, source_id: &str, target_id: &str) -> GraphEdge {
        GraphEdge {
            id: id.to_string(),
            case_id: "test-case".to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            edge_type: EdgeType::Contains,
            confidence: None,
            provenance: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_write_nodes_empty() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        create_graph_tables(&conn);
        let repo = GraphRepo::new(&conn);
        let mut writer = SqliteGraphWriter::new(repo);

        let count = writer.write_nodes(&[]).expect("write empty nodes");

        assert_eq!(count, 0);
        assert_eq!(writer.node_count, 0);
        assert_eq!(writer.edge_count, 0);
    }

    #[test]
    fn test_write_edges_empty() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        create_graph_tables(&conn);
        let repo = GraphRepo::new(&conn);
        let mut writer = SqliteGraphWriter::new(repo);

        let count = writer.write_edges(&[]).expect("write empty edges");

        assert_eq!(count, 0);
        assert_eq!(writer.node_count, 0);
        assert_eq!(writer.edge_count, 0);
    }

    #[test]
    fn test_write_nodes_single() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        create_graph_tables(&conn);
        let repo = GraphRepo::new(&conn);
        let mut writer = SqliteGraphWriter::new(repo);

        let nodes = [make_node("n1", "node-1")];
        let count = writer.write_nodes(&nodes).expect("write single node");

        assert_eq!(count, 1);
        assert_eq!(writer.node_count, 1);
        assert_eq!(writer.edge_count, 0);
    }

    #[test]
    fn test_write_edges_single() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        create_graph_tables(&conn);
        let repo = GraphRepo::new(&conn);
        let mut writer = SqliteGraphWriter::new(repo);

        // Insert referenced nodes first (FK constraint).
        let nodes = [make_node("n1", "source"), make_node("n2", "target")];
        writer.write_nodes(&nodes).expect("write nodes for edges");

        let edges = [make_edge("e1", "n1", "n2")];
        let count = writer.write_edges(&edges).expect("write single edge");

        assert_eq!(count, 1);
        assert_eq!(writer.node_count, 2);
        assert_eq!(writer.edge_count, 1);
    }

    #[test]
    fn test_write_nodes_and_edges() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        create_graph_tables(&conn);
        let repo = GraphRepo::new(&conn);
        let mut writer = SqliteGraphWriter::new(repo);

        let nodes = [
            make_node("n1", "node-1"),
            make_node("n2", "node-2"),
            make_node("n3", "node-3"),
        ];
        let node_count = writer.write_nodes(&nodes).expect("write 3 nodes");
        assert_eq!(node_count, 3);

        let edges = [make_edge("e1", "n1", "n2"), make_edge("e2", "n2", "n3")];
        let edge_count = writer.write_edges(&edges).expect("write 2 edges");
        assert_eq!(edge_count, 2);

        assert_eq!(writer.node_count, 3);
        assert_eq!(writer.edge_count, 2);
    }
}
