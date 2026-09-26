CREATE TABLE IF NOT EXISTS linux_evidence_facts (
    data_source_id TEXT PRIMARY KEY NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    facts_json TEXT NOT NULL CHECK (json_valid(facts_json)),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
