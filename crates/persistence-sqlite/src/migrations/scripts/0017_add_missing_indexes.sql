-- 添加缺失的索引以优化查询性能

-- timeline_events 表的 source_object_id 索引 (用于删除时查询)
CREATE INDEX IF NOT EXISTS idx_timeline_source_object ON timeline_events(source_object_id);

-- artifacts 表的 source_object_id 索引 (用于删除时查询)
CREATE INDEX IF NOT EXISTS idx_artifacts_source_object ON artifacts(source_object_id);

-- audit_log 表的 resource_id 索引 (用于查询特定资源的审计记录)
CREATE INDEX IF NOT EXISTS idx_audit_log_resource_id ON audit_log(resource_id);

-- file_entries 表的复合索引 (用于按类型和删除状态查询)
CREATE INDEX IF NOT EXISTS idx_file_entries_type_deleted ON file_entries(entry_type, deleted);
