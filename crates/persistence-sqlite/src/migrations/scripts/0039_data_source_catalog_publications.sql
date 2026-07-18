CREATE TABLE IF NOT EXISTS data_source_catalog_publications (
    data_source_id TEXT PRIMARY KEY NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE
        CHECK (
            length(trim(data_source_id)) > 0
            AND instr(data_source_id, char(0)) = 0
        ),
    attempt_id TEXT NOT NULL
        CHECK (
            length(trim(attempt_id)) > 0
            AND instr(attempt_id, char(0)) = 0
        ),
    input_fingerprint TEXT NOT NULL
        CHECK (
            length(input_fingerprint) = 64
            AND input_fingerprint NOT GLOB '*[^0-9a-f]*'
        ),
    source_db_rel_path TEXT NOT NULL
        CHECK (
            length(trim(source_db_rel_path)) > 0
            AND instr(source_db_rel_path, char(0)) = 0
        ),
    catalog_digest TEXT NOT NULL
        CHECK (
            length(catalog_digest) = 64
            AND catalog_digest NOT GLOB '*[^0-9a-f]*'
        ),
    seal TEXT NOT NULL
        CHECK (
            length(seal) = 64
            AND seal NOT GLOB '*[^0-9a-f]*'
        ),
    state TEXT NOT NULL DEFAULT 'prepared'
        CHECK (state IN ('prepared', 'published')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    published_at TEXT,
    CHECK (
        (state = 'prepared' AND published_at IS NULL)
        OR (state = 'published' AND published_at IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_data_source_catalog_publications_state
ON data_source_catalog_publications(state, data_source_id);
