-- Migration 0026: Correlation snapshot cache with incremental-update support.

CREATE TABLE IF NOT EXISTS correlation_snapshots (
    case_id TEXT PRIMARY KEY NOT NULL,
    snapshot_json TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    artifact_ids_json TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS correlation_edges_cache (
    edge_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    provenance TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_correlation_edges_cache_case ON correlation_edges_cache(case_id);
CREATE INDEX IF NOT EXISTS idx_correlation_edges_cache_source ON correlation_edges_cache(source_id);
CREATE INDEX IF NOT EXISTS idx_correlation_edges_cache_target ON correlation_edges_cache(target_id);
