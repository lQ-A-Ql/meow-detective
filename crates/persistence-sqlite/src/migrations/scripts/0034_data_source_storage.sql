ALTER TABLE data_sources ADD COLUMN storage_model TEXT NOT NULL DEFAULT 'source_db';
ALTER TABLE data_sources ADD COLUMN source_db_rel_path TEXT;
ALTER TABLE data_sources ADD COLUMN index_rel_path TEXT;
ALTER TABLE data_sources ADD COLUMN staging_rel_path TEXT;
ALTER TABLE data_sources ADD COLUMN platform TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE data_sources ADD COLUMN profile TEXT;
ALTER TABLE data_sources ADD COLUMN import_state TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE data_sources ADD COLUMN schema_version TEXT;
ALTER TABLE data_sources ADD COLUMN last_error TEXT;

CREATE INDEX IF NOT EXISTS idx_data_sources_import_state
ON data_sources(import_state);
