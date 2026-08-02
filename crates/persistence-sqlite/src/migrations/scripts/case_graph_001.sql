CREATE TABLE IF NOT EXISTS graph_nodes (
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

CREATE TABLE IF NOT EXISTS case_graph_projection (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    case_id TEXT NOT NULL,
    projection_version TEXT NOT NULL,
    source_manifest TEXT NOT NULL,
    built_at TEXT NOT NULL,
    source_count INTEGER NOT NULL,
    cross_source_entity_count INTEGER NOT NULL,
    cross_source_edge_count INTEGER NOT NULL,
    seed_ids_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS case_graph_sources (
    data_source_id TEXT PRIMARY KEY NOT NULL,
    schema_version TEXT NOT NULL,
    database_size_bytes INTEGER NOT NULL,
    database_modified_ns TEXT NOT NULL,
    wal_size_bytes INTEGER NOT NULL,
    wal_modified_ns TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_case_graph_nodes_case_type
ON graph_nodes(case_id, node_type, id);

CREATE INDEX IF NOT EXISTS idx_case_graph_nodes_case_created
ON graph_nodes(case_id, created_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_case_graph_edges_case
ON graph_edges(case_id);

CREATE INDEX IF NOT EXISTS idx_case_graph_edges_source
ON graph_edges(source_id, edge_type, id);

CREATE INDEX IF NOT EXISTS idx_case_graph_edges_target
ON graph_edges(target_id, edge_type, id);
