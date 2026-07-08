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

CREATE TABLE IF NOT EXISTS data_source_partitions (
    id TEXT PRIMARY KEY,
    data_source_id TEXT NOT NULL,
    partition_index INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind_label TEXT NOT NULL,
    status TEXT NOT NULL,
    type_guid TEXT,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    filesystem TEXT,
    unlock_hint TEXT,
    lvm_vg_uuid TEXT,
    lvm_vg_name TEXT,
    lvm_lv_uuid TEXT,
    lvm_lv_name TEXT,
    lvm_pv_offsets_json TEXT,
    lvm_pv_sources_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_source_partitions_data_source
ON data_source_partitions(data_source_id, partition_index);

CREATE TABLE IF NOT EXISTS file_entries (
    id TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES file_entries(id),
    data_source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT,
    hash_sha256 TEXT,
    partition_index INTEGER
);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_parent
ON file_entries(parent_id);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_data_source
ON file_entries(data_source_id);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_path
ON file_entries(path);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_parent_hidden
ON file_entries(parent_id, hidden, system);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_type_hidden
ON file_entries(entry_type, hidden, system);

CREATE INDEX IF NOT EXISTS idx_source_file_entries_type_deleted_nocase
ON file_entries(entry_type COLLATE NOCASE, deleted);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL DEFAULT '',
    data_source_id TEXT NOT NULL DEFAULT '',
    artifact_type TEXT NOT NULL,
    source_object_id TEXT,
    extractor_id TEXT,
    extractor_version TEXT,
    confidence REAL,
    source_attribution TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    attrs TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_source_artifacts_case
ON artifacts(case_id);

CREATE INDEX IF NOT EXISTS idx_source_artifacts_type
ON artifacts(artifact_type);

CREATE INDEX IF NOT EXISTS idx_source_artifacts_source
ON artifacts(source_object_id);

CREATE INDEX IF NOT EXISTS idx_source_artifacts_data_source
ON artifacts(data_source_id);

CREATE TABLE IF NOT EXISTS timeline_events (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL DEFAULT '',
    source_object_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    ts TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    parser_id TEXT,
    parser_version TEXT,
    confidence REAL,
    source_attribution TEXT,
    attrs TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_source_timeline_case_ts
ON timeline_events(case_id, ts);

CREATE INDEX IF NOT EXISTS idx_source_timeline_type
ON timeline_events(event_type);

CREATE INDEX IF NOT EXISTS idx_source_timeline_source
ON timeline_events(source_object_id);

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
