pub(super) const CATALOG: &str = "SELECT id, parent_id, path, name, entry_type, size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256, partition_index FROM file_entries";
pub(super) const NODES: &str = "SELECT id, node_type, label, summary, tags FROM graph_nodes";
pub(super) const EDGES: &str =
    "SELECT id, source_id, target_id, edge_type, confidence, provenance FROM graph_edges";
pub(super) const TIMELINE: &str = "SELECT id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs FROM timeline_events";
