CREATE TABLE IF NOT EXISTS source_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS data_sources (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    source_hash_sha256 TEXT,
    hash_status TEXT DEFAULT 'unknown',
    canonical_source_path TEXT,
    evidence_size INTEGER,
    reader_kind TEXT,
    provenance_status TEXT DEFAULT 'unknown',
    provenance_warnings TEXT DEFAULT '[]'
);

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
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_source_graph_nodes_case
ON graph_nodes(case_id);

CREATE INDEX IF NOT EXISTS idx_source_graph_edges_case
ON graph_edges(case_id);
