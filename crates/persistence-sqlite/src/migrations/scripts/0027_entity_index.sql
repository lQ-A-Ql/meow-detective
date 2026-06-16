-- Migration 0027: Entity pre-normalization index for deduplication and fast lookup.

CREATE TABLE IF NOT EXISTS entity_index (
    value_hash TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    value_normalized TEXT NOT NULL,
    source_artifact_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (value_hash, entity_type)
);

CREATE INDEX IF NOT EXISTS idx_entity_index_hash_type ON entity_index(value_hash, entity_type);
