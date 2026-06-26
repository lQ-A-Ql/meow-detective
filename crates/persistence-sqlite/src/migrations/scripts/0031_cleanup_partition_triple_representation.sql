-- 0031_cleanup_partition_triple_representation
--
-- 修复分区三重表示问题：
-- 1. data_source_partitions 表（0009）是唯一的分区数据源标准表
-- 2. 废弃 partitions 表（0013 创建），清空数据
-- 3. 清理 migration_log 中永久 pending 的记录

-- Step 1: 废弃 partitions 表（0013 创建），清空其中可能残留的数据
--（保留表结构以防需要回滚，但不写入任何数据）
DELETE FROM partitions;

-- Step 2: 清理 migration_log 中永久 pending 的条目
--（0014_migrate_partitions 从未完成，被 0031 取代）
DELETE FROM migration_log WHERE migration_name = '0014_migrate_partitions';

-- Step 3: 确保 data_source_partitions 索引存在
CREATE INDEX IF NOT EXISTS idx_data_source_partitions_data_source
ON data_source_partitions(data_source_id, partition_index);
