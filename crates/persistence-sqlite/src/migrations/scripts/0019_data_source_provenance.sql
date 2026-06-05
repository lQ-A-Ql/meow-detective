ALTER TABLE data_sources ADD COLUMN source_hash_sha256 TEXT;
ALTER TABLE data_sources ADD COLUMN hash_status TEXT DEFAULT 'unknown';
ALTER TABLE data_sources ADD COLUMN canonical_source_path TEXT;
ALTER TABLE data_sources ADD COLUMN evidence_size INTEGER;
ALTER TABLE data_sources ADD COLUMN reader_kind TEXT;
ALTER TABLE data_sources ADD COLUMN provenance_status TEXT DEFAULT 'unknown';
ALTER TABLE data_sources ADD COLUMN provenance_warnings TEXT DEFAULT '[]';
