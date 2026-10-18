use std::time::Duration;

/// Event/graph wall times also include bookkeeping. Node/edge/page durations
/// include their prepare subspans; prepare timings must not be summed twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineWriteTimings {
    pub graph_node_prepare: Duration,
    pub graph_edge_prepare: Duration,
    pub graph_page_prepare: Duration,
    pub graph_buffer_rows: u64,
    pub graph_buffer_bytes: u64,
    pub graph_wal_sampled_bytes: u64,
    pub event_clear: Duration,
    pub event_cursor: Duration,
    pub event_insert: Duration,
    pub event_indexes: Duration,
    pub event_commit: Duration,
    pub graph_page: Duration,
    pub graph_nodes: Duration,
    pub graph_edges: Duration,
    pub graph_commit: Duration,
}
