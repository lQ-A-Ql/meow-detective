CREATE INDEX IF NOT EXISTS idx_source_artifacts_created_id
ON artifacts(created_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_source_artifacts_type_created_id
ON artifacts(artifact_type, created_at DESC, id ASC);
