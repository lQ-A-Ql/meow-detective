use std::time::Duration;

/// Exclusive write timings. Event/graph wall times also include bookkeeping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineWriteTimings {
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
