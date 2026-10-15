CREATE TABLE evidence_tasks (
    task_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('digest','nestedImport','contentIndex','registrySearch')),
    state TEXT NOT NULL CHECK(state IN ('queued','running','cancelling','completed','failed','cancelled')),
    request_json TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    bytes_processed INTEGER NOT NULL DEFAULT 0 CHECK(bytes_processed >= 0),
    total_bytes INTEGER NOT NULL DEFAULT 0 CHECK(total_bytes >= 0),
    result_json TEXT,
    error_code TEXT,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_evidence_tasks_active ON evidence_tasks(case_id,fingerprint)
WHERE state IN ('queued','running','cancelling');
CREATE INDEX idx_evidence_tasks_completed ON evidence_tasks(case_id,fingerprint,state);

-- Checkpoints belong to an evidence revision, so a failed attempt can continue
-- without making its unpublished generation visible to ordinary searches.
CREATE TABLE content_index_checkpoints (
    fingerprint TEXT PRIMARY KEY,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    source_revision TEXT NOT NULL,
    after_id TEXT NOT NULL DEFAULT '',
    summary_json TEXT NOT NULL,
    published INTEGER NOT NULL DEFAULT 0 CHECK(published IN (0,1))
);
CREATE TABLE content_index_entries (
    fingerprint TEXT NOT NULL REFERENCES content_index_checkpoints(fingerprint) ON DELETE CASCADE,
    file_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('indexed','binary','oversized','unreadable','unsupportedEncoding','notApplicable')),
    PRIMARY KEY(fingerprint,file_id)
);
