-- 0032_file_entry_type_nocase_index
--
-- 添加 NOCASE 索引以优化 entry_type + deleted 的目录过滤查询。
-- 旧的 BINARY 索引 idx_file_entries_type_deleted 在某些 SQLite 版本上
-- 会导致全表扫描（因为 'directory' 的 BINARY 比较需要精确匹配大小写）。

CREATE INDEX IF NOT EXISTS idx_file_entries_type_deleted_nocase
ON file_entries(entry_type COLLATE NOCASE, deleted);

-- 旧索引仍保留以支持依赖 BINARY 排序的查询（如有）。
-- 未来可通过 EXPLAIN 确认无查询使用旧索引后 DROP。
