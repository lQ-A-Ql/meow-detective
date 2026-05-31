-- 迁移现有分区数据到 partitions 表
-- 注意：此脚本需要在应用层执行，因为需要解析 JSON 数据

-- 首先检查 data_sources 表是否有 partitions 列
-- 如果有，则需要在应用层解析 JSON 并插入到 partitions 表

-- 创建临时表用于记录迁移状态
CREATE TABLE IF NOT EXISTS migration_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    migration_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    details TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO migration_log (migration_name, status, details)
VALUES ('0012_migrate_partitions', 'pending', 'Waiting for application-layer migration');
