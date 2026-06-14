-- Migration 0025: Batch processing subsystem (batch jobs, phases, checkpoints).

CREATE TABLE IF NOT EXISTS batch_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    plan_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'queued',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS batch_phases (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued',
    progress REAL NOT NULL DEFAULT 0.0,
    started_at TEXT,
    completed_at TEXT,
    error_count INTEGER NOT NULL DEFAULT 0,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    UNIQUE(batch_id, kind),
    FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS batch_checkpoints (
    batch_id TEXT NOT NULL,
    phase_kind TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL DEFAULT '{}',
    saved_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (batch_id, phase_kind, key),
    FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_batch_jobs_case ON batch_jobs(case_id);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_status ON batch_jobs(status);
CREATE INDEX IF NOT EXISTS idx_batch_phases_batch ON batch_phases(batch_id);
CREATE INDEX IF NOT EXISTS idx_batch_phases_state ON batch_phases(state);
CREATE INDEX IF NOT EXISTS idx_batch_checkpoints_batch ON batch_checkpoints(batch_id);
