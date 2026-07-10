CREATE TABLE IF NOT EXISTS data_source_clusters (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    platform TEXT NOT NULL DEFAULT 'linux',
    profile TEXT,
    manifest_rel_path TEXT NOT NULL,
    import_state TEXT NOT NULL DEFAULT 'pending',
    member_count INTEGER NOT NULL DEFAULT 0,
    ready_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE data_sources ADD COLUMN cluster_id TEXT;
ALTER TABLE data_sources ADD COLUMN cluster_member_index INTEGER;
ALTER TABLE data_sources ADD COLUMN cluster_member_count INTEGER;

CREATE INDEX IF NOT EXISTS idx_data_source_clusters_case
ON data_source_clusters(case_id);

CREATE INDEX IF NOT EXISTS idx_data_sources_cluster
ON data_sources(cluster_id, cluster_member_index);
