CREATE TABLE data_sources (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id),
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_data_sources_case_id ON data_sources(case_id);
