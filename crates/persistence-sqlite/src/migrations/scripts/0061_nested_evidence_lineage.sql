CREATE TABLE IF NOT EXISTS nested_evidence_lineage (
    parent_data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    nested_file_path TEXT NOT NULL,
    derived_data_source_id TEXT REFERENCES data_sources(id) ON DELETE SET NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    probe_kind TEXT NOT NULL,
    PRIMARY KEY (parent_data_source_id, nested_file_path),
    CHECK (length > 0 AND offset >= 0 AND instr(nested_file_path, char(0)) = 0),
    CHECK (length <= 9223372036854775807 AND offset <= 9223372036854775807)
);
CREATE INDEX IF NOT EXISTS idx_nested_lineage_derived
ON nested_evidence_lineage(derived_data_source_id);
