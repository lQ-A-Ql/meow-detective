CREATE TABLE IF NOT EXISTS linux_import_sets (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    import_state TEXT NOT NULL DEFAULT 'pending' CHECK (import_state IN (
        'pending', 'importing', 'ready', 'failed', 'cancelled'
    )),
    member_count INTEGER NOT NULL CHECK (member_count > 0),
    ready_count INTEGER NOT NULL DEFAULT 0 CHECK (ready_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS linux_import_set_members (
    import_set_id TEXT NOT NULL REFERENCES linux_import_sets(id) ON DELETE CASCADE,
    member_index INTEGER NOT NULL CHECK (member_index >= 0),
    source_path TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    data_source_id TEXT REFERENCES data_sources(id) ON DELETE SET NULL,
    import_state TEXT NOT NULL DEFAULT 'pending' CHECK (import_state IN (
        'pending', 'importing', 'ready', 'failed', 'cancelled'
    )),
    last_error TEXT,
    PRIMARY KEY (import_set_id, member_index),
    UNIQUE (import_set_id, source_path)
);

CREATE INDEX IF NOT EXISTS idx_linux_import_set_members_source
ON linux_import_set_members(data_source_id);
