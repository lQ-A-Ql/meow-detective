CREATE TABLE forensic_fingerprints (
    object_key TEXT PRIMARY KEY NOT NULL,
    object_id TEXT NOT NULL,
    case_id TEXT,
    source_id TEXT,
    object_type TEXT NOT NULL CHECK (object_type IN ('data_source', 'file_entry', 'artifact', 'report')),
    parent_object_id TEXT,
    source_locator TEXT,
    byte_length INTEGER,
    content_sha256 TEXT CHECK (
        content_sha256 IS NULL
        OR (length(content_sha256) = 64 AND content_sha256 NOT GLOB '*[^0-9a-f]*')
    ),
    metadata_sha256 TEXT NOT NULL CHECK (
        length(metadata_sha256) = 64 AND metadata_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    parser_id TEXT,
    parser_version TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_forensic_fingerprints_case
    ON forensic_fingerprints(case_id, object_type, object_id);
CREATE INDEX idx_forensic_fingerprints_source
    ON forensic_fingerprints(source_id, object_type, object_id);
