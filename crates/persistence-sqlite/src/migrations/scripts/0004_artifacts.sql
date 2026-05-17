CREATE TABLE artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL DEFAULT '',
    data_source_id TEXT NOT NULL DEFAULT '',
    artifact_type TEXT NOT NULL,
    source_object_id TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    attrs TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_artifacts_case ON artifacts(case_id);
CREATE INDEX idx_artifacts_type ON artifacts(artifact_type);
CREATE INDEX idx_artifacts_source ON artifacts(source_object_id);
