-- Migration 0028: Entity merge tables for canonicalization and deduplication.
-- Note: entity_merge_log does not enforce FK on kept/merged entity IDs
-- because merged entities are deleted during deduplication and the log
-- must survive for auditability.

CREATE TABLE IF NOT EXISTS entity_merge_log (
    merge_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    kept_entity_id TEXT NOT NULL,
    merged_entity_id TEXT NOT NULL,
    confidence REAL NOT NULL,
    merged_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS resolved_entities (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    canonical_value TEXT NOT NULL,
    source_count INTEGER NOT NULL DEFAULT 1,
    confidence REAL NOT NULL DEFAULT 0.70,
    attributes_json TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_entity_merge_log_case ON entity_merge_log(case_id);
CREATE INDEX IF NOT EXISTS idx_resolved_entities_case ON resolved_entities(case_id);
CREATE INDEX IF NOT EXISTS idx_resolved_entities_type ON resolved_entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_resolved_entities_canonical ON resolved_entities(canonical_value);
