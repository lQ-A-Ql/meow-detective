CREATE INDEX IF NOT EXISTS idx_source_file_entries_analysis_feed
ON file_entries(data_source_id, path ASC, id ASC)
WHERE LOWER(entry_type) = 'file';
