-- 添加复合索引以优化常用查询

-- file_entries 复合索引
CREATE INDEX IF NOT EXISTS idx_file_entries_type_deleted ON file_entries(entry_type, deleted);
CREATE INDEX IF NOT EXISTS idx_file_entries_hash ON file_entries(hash_sha256) WHERE hash_sha256 IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_file_entries_size ON file_entries(size);

-- timeline_events 复合索引
CREATE INDEX IF NOT EXISTS idx_timeline_case_type_ts ON timeline_events(case_id, event_type, ts);

-- artifacts 复合索引
CREATE INDEX IF NOT EXISTS idx_artifacts_case_type ON artifacts(case_id, artifact_type);
