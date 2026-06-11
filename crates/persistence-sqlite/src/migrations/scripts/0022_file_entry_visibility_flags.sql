ALTER TABLE file_entries ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
ALTER TABLE file_entries ADD COLUMN system INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_file_entries_parent_hidden
ON file_entries(parent_id, hidden, system);

CREATE INDEX IF NOT EXISTS idx_file_entries_type_hidden
ON file_entries(entry_type, hidden, system);
