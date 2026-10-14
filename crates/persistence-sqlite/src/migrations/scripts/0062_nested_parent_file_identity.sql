ALTER TABLE nested_evidence_lineage ADD COLUMN parent_file_id TEXT;
CREATE INDEX IF NOT EXISTS idx_nested_lineage_parent_file
ON nested_evidence_lineage(parent_data_source_id, parent_file_id);

CREATE TABLE IF NOT EXISTS evidence_digests (
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    data_source_id TEXT NOT NULL DEFAULT '',
    file_id TEXT NOT NULL DEFAULT '',
    partition_index INTEGER NOT NULL DEFAULT -1,
    value TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    status TEXT NOT NULL CHECK (status IN ('completed', 'cancelled', 'failed')),
    source_revision TEXT NOT NULL,
    computed_at TEXT NOT NULL,
    PRIMARY KEY (case_id, scope, algorithm, data_source_id, file_id, partition_index)
);
CREATE INDEX IF NOT EXISTS idx_evidence_digests_target
ON evidence_digests(case_id, data_source_id, file_id);
