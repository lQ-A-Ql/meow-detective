CREATE INDEX IF NOT EXISTS idx_source_file_entries_mount_children
ON file_entries(
    parent_id,
    data_source_id,
    partition_index,
    entry_type,
    name COLLATE NOCASE,
    id
);
