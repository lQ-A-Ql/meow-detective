CREATE INDEX IF NOT EXISTS idx_source_graph_nodes_case_created_id
ON graph_nodes(case_id, created_at DESC, id ASC);
