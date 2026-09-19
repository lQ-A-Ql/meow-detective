CREATE TABLE forensic_ledger (
    id TEXT PRIMARY KEY NOT NULL,
    scope_key TEXT NOT NULL,
    case_id TEXT,
    audit_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT NOT NULL,
    previous_hash TEXT NOT NULL CHECK (length(previous_hash) = 64 AND previous_hash NOT GLOB '*[^0-9a-f]*'),
    entry_hash TEXT NOT NULL CHECK (length(entry_hash) = 64 AND entry_hash NOT GLOB '*[^0-9a-f]*'),
    created_at TEXT NOT NULL,
    UNIQUE(scope_key, sequence),
    UNIQUE(scope_key, entry_hash)
);

CREATE INDEX idx_forensic_ledger_case ON forensic_ledger(case_id, sequence);
CREATE INDEX idx_forensic_ledger_scope ON forensic_ledger(scope_key, sequence);
CREATE INDEX idx_forensic_ledger_audit ON forensic_ledger(audit_id);
