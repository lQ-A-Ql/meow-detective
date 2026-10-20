CREATE INDEX IF NOT EXISTS idx_source_file_entries_browse_parent_visibility_name
ON file_entries(
    parent_id,
    hidden,
    system,
    entry_type COLLATE NOCASE,
    name COLLATE NOCASE,
    id
);
