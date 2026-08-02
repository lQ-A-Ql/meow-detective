CREATE INDEX IF NOT EXISTS idx_source_graph_nodes_case_type_id
ON graph_nodes(case_id, node_type, id);
