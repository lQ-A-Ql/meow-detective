-- Chain-of-custody table for tamper-evident audit logging.
-- Each row is one link in a sequentially-hashed custody chain.

CREATE TABLE IF NOT EXISTS chain_of_custody (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    action TEXT NOT NULL,
    actor TEXT NOT NULL DEFAULT 'system',
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    prev_entry_hash TEXT NOT NULL DEFAULT '',
    data_hash TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_custody_case ON chain_of_custody(case_id);
CREATE INDEX IF NOT EXISTS idx_custody_timestamp ON chain_of_custody(timestamp);
CREATE INDEX IF NOT EXISTS idx_custody_action ON chain_of_custody(action);
