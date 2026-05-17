CREATE TABLE reports (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id),
    template_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    created_by TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'running',
    progress INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_reports_case ON reports(case_id);
