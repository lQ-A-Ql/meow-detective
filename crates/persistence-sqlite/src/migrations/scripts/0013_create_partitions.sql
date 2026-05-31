-- 创建 partitions 表，规范化存储分区信息

CREATE TABLE IF NOT EXISTS partitions (
    id TEXT PRIMARY KEY NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    partition_index INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind_label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unsupported',
    type_guid TEXT,
    offset INTEGER NOT NULL DEFAULT 0,
    length INTEGER NOT NULL DEFAULT 0,
    filesystem TEXT,
    unlock_hint TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_partitions_data_source ON partitions(data_source_id);
CREATE INDEX IF NOT EXISTS idx_partitions_status ON partitions(status);
CREATE INDEX IF NOT EXISTS idx_partitions_index ON partitions(data_source_id, partition_index);
