CREATE TABLE nested_range_sources (
    source_id TEXT PRIMARY KEY REFERENCES data_sources(id) ON DELETE CASCADE,
    parent_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    file_id TEXT NOT NULL CHECK (length(file_id) > 0),
    parent_revision TEXT NOT NULL CHECK (length(parent_revision) > 0),
    payload_length INTEGER NOT NULL CHECK (payload_length > 0 AND payload_length % 512 = 0),
    CHECK (source_id <> parent_id)
);
CREATE INDEX idx_nested_range_parent ON nested_range_sources(parent_id);

DELETE FROM data_sources WHERE kind = 'raw' AND profile = 'nested_vhd';
DELETE FROM nested_evidence_lineage WHERE probe_kind = 'vhd-fixed';
