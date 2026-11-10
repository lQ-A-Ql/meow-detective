-- audit_log 表的 resource_id 索引 (用于查询特定资源的审计记录)
CREATE INDEX IF NOT EXISTS idx_audit_log_resource_id ON audit_log(resource_id);
