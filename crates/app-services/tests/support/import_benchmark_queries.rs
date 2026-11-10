pub(super) const NODES: &str = "SELECT id, node_type, label, summary, tags FROM graph_nodes";
pub(super) const EDGES: &str =
    "SELECT id, source_id, target_id, edge_type, confidence, provenance FROM graph_edges";
