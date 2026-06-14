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
