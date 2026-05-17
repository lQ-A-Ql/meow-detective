CREATE TABLE cases (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    number TEXT,
    examiner TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
