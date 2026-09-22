ALTER TABLE linux_import_set_members RENAME TO linux_import_set_members_old;

CREATE TABLE linux_import_set_members (
    import_set_id TEXT NOT NULL REFERENCES linux_import_sets(id) ON DELETE CASCADE,
    member_index INTEGER NOT NULL CHECK (member_index >= 0),
    source_path TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    data_source_id TEXT REFERENCES data_sources(id) ON DELETE SET NULL,
    import_state TEXT NOT NULL DEFAULT 'pending' CHECK (import_state IN (
        'pending', 'importing', 'ready', 'ready_metadata', 'failed', 'cancelled'
    )),
    last_error TEXT,
    PRIMARY KEY (import_set_id, member_index),
    UNIQUE (import_set_id, source_path)
);

INSERT INTO linux_import_set_members (
    import_set_id, member_index, source_path, source_kind,
    data_source_id, import_state, last_error
)
SELECT import_set_id, member_index, source_path, source_kind,
       data_source_id, import_state, last_error
FROM linux_import_set_members_old;

DROP TABLE linux_import_set_members_old;

CREATE INDEX idx_linux_import_set_members_source
ON linux_import_set_members(data_source_id);
