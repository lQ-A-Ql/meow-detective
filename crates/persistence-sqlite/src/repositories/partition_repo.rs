use crate::connection::DbResult;
use rusqlite::{params, Connection};

/// 分区记录
#[derive(Debug, Clone)]
pub struct DataSourcePartitionRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub name: String,
    pub kind_label: String,
    pub status: String,
    pub type_guid: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub filesystem: Option<String>,
    pub unlock_hint: Option<String>,
}

/// 分区仓库
pub struct PartitionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PartitionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入单条分区记录
    pub fn insert(&self, record: &DataSourcePartitionRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.id,
                record.data_source_id,
                record.partition_index,
                record.name,
                record.kind_label,
                record.status,
                record.type_guid,
                record.offset,
                record.length,
                record.filesystem,
                record.unlock_hint,
            ],
        )?;
        Ok(())
    }

    /// 批量插入分区记录
    pub fn insert_batch(&self, records: &[DataSourcePartitionRecord]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for record in records {
                stmt.execute(params![
                    record.id,
                    record.data_source_id,
                    record.partition_index,
                    record.name,
                    record.kind_label,
                    record.status,
                    record.type_guid,
                    record.offset,
                    record.length,
                    record.filesystem,
                    record.unlock_hint,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 替换数据源的所有分区（删除旧的，插入新的）
    pub fn replace_for_data_source(
        &self,
        data_source_id: &str,
        records: &[DataSourcePartitionRecord],
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        
        // 删除旧分区
        tx.execute(
            "DELETE FROM partitions WHERE data_source_id = ?1",
            params![data_source_id],
        )?;

        // 插入新分区
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO partitions (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for record in records {
                stmt.execute(params![
                    record.id,
                    record.data_source_id,
                    record.partition_index,
                    record.name,
                    record.kind_label,
                    record.status,
                    record.type_guid,
                    record.offset,
                    record.length,
                    record.filesystem,
                    record.unlock_hint,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// 按数据源查询分区
    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<DataSourcePartitionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint
             FROM partitions
             WHERE data_source_id = ?1
             ORDER BY partition_index ASC",
        )?;
        let rows = stmt.query_map(params![data_source_id], |row| {
            Ok(DataSourcePartitionRecord {
                id: row.get(0)?,
                data_source_id: row.get(1)?,
                partition_index: row.get(2)?,
                name: row.get(3)?,
                kind_label: row.get(4)?,
                status: row.get(5)?,
                type_guid: row.get(6)?,
                offset: row.get(7)?,
                length: row.get(8)?,
                filesystem: row.get(9)?,
                unlock_hint: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 删除数据源的所有分区
    pub fn delete_by_data_source(&self, data_source_id: &str) -> DbResult<usize> {
        let count = self.conn.execute(
            "DELETE FROM partitions WHERE data_source_id = ?1",
            params![data_source_id],
        )?;
        Ok(count)
    }

    /// 统计数据源的分区数量
    pub fn count_by_data_source(&self, data_source_id: &str) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM partitions WHERE data_source_id = ?1",
            params![data_source_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// 按状态查询分区
    pub fn find_by_status(&self, status: &str) -> DbResult<Vec<DataSourcePartitionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint
             FROM partitions
             WHERE status = ?1
             ORDER BY data_source_id, partition_index",
        )?;
        let rows = stmt.query_map(params![status], |row| {
            Ok(DataSourcePartitionRecord {
                id: row.get(0)?,
                data_source_id: row.get(1)?,
                partition_index: row.get(2)?,
                name: row.get(3)?,
                kind_label: row.get(4)?,
                status: row.get(5)?,
                type_guid: row.get(6)?,
                offset: row.get(7)?,
                length: row.get(8)?,
                filesystem: row.get(9)?,
                unlock_hint: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 更新分区状态
    pub fn update_status(&self, id: &str, status: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE partitions SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::open_in_memory;

    fn create_test_record(ds_id: &str, index: u32) -> DataSourcePartitionRecord {
        DataSourcePartitionRecord {
            id: format!("partition-{}", index),
            data_source_id: ds_id.to_string(),
            partition_index: index,
            name: format!("Partition {}", index),
            kind_label: "NTFS".to_string(),
            status: "supported".to_string(),
            type_guid: None,
            offset: 0,
            length: 1024 * 1024,
            filesystem: Some("NTFS".to_string()),
            unlock_hint: None,
        }
    }

    #[test]
    fn test_insert_and_query() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
             CREATE TABLE partitions (id TEXT PRIMARY KEY, data_source_id TEXT REFERENCES data_sources(id), partition_index INTEGER, name TEXT, kind_label TEXT, status TEXT, type_guid TEXT, offset INTEGER, length INTEGER, filesystem TEXT, unlock_hint TEXT, created_at TEXT DEFAULT (datetime('now')));"
        ).unwrap();

        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES ('ds1', 'c1', 'Test', 'E01', '/path', '2024-01-01')",
            [],
        ).unwrap();

        let repo = PartitionRepo::new(&conn);
        let record = create_test_record("ds1", 1);
        repo.insert(&record).unwrap();

        let records = repo.find_by_data_source("ds1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "Partition 1");
    }

    #[test]
    fn test_replace_for_data_source() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
             CREATE TABLE partitions (id TEXT PRIMARY KEY, data_source_id TEXT REFERENCES data_sources(id), partition_index INTEGER, name TEXT, kind_label TEXT, status TEXT, type_guid TEXT, offset INTEGER, length INTEGER, filesystem TEXT, unlock_hint TEXT, created_at TEXT DEFAULT (datetime('now')));"
        ).unwrap();

        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES ('ds1', 'c1', 'Test', 'E01', '/path', '2024-01-01')",
            [],
        ).unwrap();

        let repo = PartitionRepo::new(&conn);

        // 插入初始分区
        let records = vec![
            create_test_record("ds1", 1),
            create_test_record("ds1", 2),
        ];
        repo.replace_for_data_source("ds1", &records).unwrap();

        let result = repo.find_by_data_source("ds1").unwrap();
        assert_eq!(result.len(), 2);

        // 替换为新分区
        let new_records = vec![create_test_record("ds1", 3)];
        repo.replace_for_data_source("ds1", &new_records).unwrap();

        let result = repo.find_by_data_source("ds1").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].partition_index, 3);
    }

    #[test]
    fn test_count_by_data_source() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (id TEXT PRIMARY KEY, case_id TEXT, name TEXT, kind TEXT, source_path TEXT, imported_at TEXT);
             CREATE TABLE partitions (id TEXT PRIMARY KEY, data_source_id TEXT REFERENCES data_sources(id), partition_index INTEGER, name TEXT, kind_label TEXT, status TEXT, type_guid TEXT, offset INTEGER, length INTEGER, filesystem TEXT, unlock_hint TEXT, created_at TEXT DEFAULT (datetime('now')));"
        ).unwrap();

        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES ('ds1', 'c1', 'Test', 'E01', '/path', '2024-01-01')",
            [],
        ).unwrap();

        let repo = PartitionRepo::new(&conn);
        assert_eq!(repo.count_by_data_source("ds1").unwrap(), 0);

        let records = vec![
            create_test_record("ds1", 1),
            create_test_record("ds1", 2),
        ];
        repo.insert_batch(&records).unwrap();

        assert_eq!(repo.count_by_data_source("ds1").unwrap(), 2);
    }
}
