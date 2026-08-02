mod manifest;
mod projection;
mod traversal;

pub(super) use projection::ensure_case_graph;
pub(super) use traversal::query_case_graph;

pub(super) const CASE_GRAPH_PROJECTION_VERSION: &str = "case-graph-exact-entity-v1";
