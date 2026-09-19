CREATE TABLE forensic_ledger_batches (
    id TEXT PRIMARY KEY NOT NULL,
    scope_key TEXT NOT NULL,
    case_id TEXT,
    start_sequence INTEGER NOT NULL CHECK (start_sequence > 0),
    end_sequence INTEGER NOT NULL CHECK (end_sequence >= start_sequence),
    entry_count INTEGER NOT NULL CHECK (entry_count > 0),
    merkle_root TEXT NOT NULL CHECK (length(merkle_root) = 64 AND merkle_root NOT GLOB '*[^0-9a-f]*'),
    head_hash TEXT NOT NULL CHECK (length(head_hash) = 64 AND head_hash NOT GLOB '*[^0-9a-f]*'),
    created_at TEXT NOT NULL,
    UNIQUE(scope_key, start_sequence, end_sequence),
    UNIQUE(scope_key, merkle_root)
);

CREATE INDEX idx_forensic_ledger_batches_scope
    ON forensic_ledger_batches(scope_key, start_sequence);
CREATE INDEX idx_forensic_ledger_batches_case
    ON forensic_ledger_batches(case_id, start_sequence);
