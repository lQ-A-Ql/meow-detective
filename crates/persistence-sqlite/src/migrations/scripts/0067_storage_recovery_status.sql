CREATE TABLE storage_recovery_status (
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    scope_key TEXT NOT NULL,
    source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    operation_id TEXT REFERENCES storage_operations(operation_id) ON DELETE SET NULL,
    disposition TEXT NOT NULL CHECK (disposition IN ('ready', 'recovery_required', 'repair_required')),
    storage_state TEXT,
    detail TEXT NOT NULL,
    scanned_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (case_id, scope_key)
);

CREATE INDEX idx_storage_recovery_status_case_disposition
    ON storage_recovery_status(case_id, disposition, scanned_at);

CREATE INDEX idx_storage_recovery_status_source
    ON storage_recovery_status(source_id, disposition);
