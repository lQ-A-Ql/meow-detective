-- Migration 0029: Entity relationships table for inferred entity-to-entity relationships.
-- Stores inferred relationships (CommunicatesWith, Owns, LoggedInto, Executed,
-- Downloaded, Accessed) between resolved entities, derived from graph edge patterns.

CREATE TABLE IF NOT EXISTS entity_relationships (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    source_entity_id TEXT NOT NULL,
    target_entity_id TEXT NOT NULL,
    relationship_type TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.70,
    evidence_edge_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_entity_rel_source_type ON entity_relationships(source_entity_id, relationship_type);
CREATE INDEX IF NOT EXISTS idx_entity_rel_case ON entity_relationships(case_id);
