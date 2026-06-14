-- Migration 0024: Investigative notebook entries, evidence citations, and investigation steps.

CREATE TABLE IF NOT EXISTS notebook_entries (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    parent_id TEXT,
    author TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    title TEXT NOT NULL,
    body_markdown TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES notebook_entries(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS evidence_citations (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL,
    target_node_type TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    display_label TEXT NOT NULL,
    snippet TEXT,
    cited_at TEXT NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES notebook_entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS investigation_steps (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    step_kind TEXT NOT NULL,
    params_json TEXT NOT NULL DEFAULT '{}',
    timestamp TEXT NOT NULL,
    duration_ms INTEGER,
    case_state_hash TEXT,
    success INTEGER,
    error_code TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notebook_entries_case ON notebook_entries(case_id);
CREATE INDEX IF NOT EXISTS idx_notebook_entries_parent ON notebook_entries(parent_id);
CREATE INDEX IF NOT EXISTS idx_notebook_entries_type ON notebook_entries(entry_type);
CREATE INDEX IF NOT EXISTS idx_notebook_entries_status ON notebook_entries(status);
CREATE INDEX IF NOT EXISTS idx_notebook_entries_created ON notebook_entries(created_at);

CREATE INDEX IF NOT EXISTS idx_citations_entry ON evidence_citations(entry_id);
CREATE INDEX IF NOT EXISTS idx_citations_target_type ON evidence_citations(target_node_type, target_node_id);

CREATE INDEX IF NOT EXISTS idx_investigation_steps_case ON investigation_steps(case_id);
CREATE INDEX IF NOT EXISTS idx_investigation_steps_kind ON investigation_steps(step_kind);
CREATE INDEX IF NOT EXISTS idx_investigation_steps_timestamp ON investigation_steps(timestamp);
