mod manifest;
mod projection;
mod traversal;

pub(super) use projection::{case_graph_exists, ensure_case_graph, open_case_graph_if_current};
pub(super) use traversal::{query_case_graph, query_case_graph_read_only};

pub(super) const CASE_GRAPH_PROJECTION_VERSION: &str = "case-graph-exact-entity-v1";
