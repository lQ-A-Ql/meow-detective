CREATE TABLE IF NOT EXISTS kubernetes_analysis_runs (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    import_set_id TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed')),
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    expected_member_count INTEGER NOT NULL DEFAULT 0,
    parsed_artifact_count INTEGER NOT NULL DEFAULT 0,
    failed_artifact_count INTEGER NOT NULL DEFAULT 0,
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    started_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_kubernetes_analysis_runs_case
ON kubernetes_analysis_runs(case_id, import_set_id, started_at);

CREATE TABLE IF NOT EXISTS kubernetes_analysis_artifacts (
    run_id TEXT NOT NULL REFERENCES kubernetes_analysis_runs(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    file_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('parsed', 'candidate', 'failed')),
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    content_digest TEXT,
    PRIMARY KEY (run_id, data_source_id, file_id)
);

CREATE INDEX IF NOT EXISTS idx_kubernetes_analysis_artifacts_run
ON kubernetes_analysis_artifacts(run_id, status);
