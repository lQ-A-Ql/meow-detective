use crate::connection::DbResult;
use rusqlite::{params, Connection};

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

pub struct PartitionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PartitionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_for_data_source(
        &self,
        data_source_id: &str,
        records: &[DataSourcePartitionRecord],
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM data_source_partitions WHERE data_source_id = ?1",
            params![data_source_id],
        )?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO data_source_partitions
                 (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
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

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<DataSourcePartitionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint
             FROM data_source_partitions
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

    /// 批量插入分区记录
    pub fn insert_batch(&self, records: &[DataSourcePartitionRecord]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO data_source_partitions
                 (id, data_source_id, partition_index, name, kind_label, status, type_guid, offset, length, filesystem, unlock_hint)
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

    /// 统计数据源的分区数量
    pub fn count_by_data_source(&self, data_source_id: &str) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM data_source_partitions WHERE data_source_id = ?1",
            params![data_source_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// 删除数据源的所有分区
    pub fn delete_by_data_source(&self, data_source_id: &str) -> DbResult<usize> {
        let count = self.conn.execute(
            "DELETE FROM data_source_partitions WHERE data_source_id = ?1",
            params![data_source_id],
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_source_partitions (
                id TEXT PRIMARY KEY,
                data_source_id TEXT NOT NULL,
                partition_index INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind_label TEXT NOT NULL,
                status TEXT NOT NULL,
                type_guid TEXT,
                offset INTEGER NOT NULL,
                length INTEGER NOT NULL,
                filesystem TEXT,
                unlock_hint TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn make_partition(id: &str, ds_id: &str, index: u32, name: &str) -> DataSourcePartitionRecord {
        DataSourcePartitionRecord {
            id: id.to_string(),
            data_source_id: ds_id.to_string(),
            partition_index: index,
            name: name.to_string(),
            kind_label: "GPT".to_string(),
            status: "ok".to_string(),
            type_guid: None,
            offset: 2048,
            length: 1024000,
            filesystem: Some("NTFS".to_string()),
            unlock_hint: None,
        }
    }

    #[test]
    fn insert_batch_then_find_by_data_source() {
        let conn = setup_db();
        let repo = PartitionRepo::new(&conn);
        let records = vec![
            make_partition("p1", "ds-1", 0, "Partition 1"),
            make_partition("p2", "ds-1", 1, "Partition 2"),
        ];
        repo.insert_batch(&records).unwrap();

        let found = repo.find_by_data_source("ds-1").unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Partition 1");
        assert_eq!(found[1].name, "Partition 2");
    }

    #[test]
    fn count_by_data_source_returns_correct_count() {
        let conn = setup_db();
        let repo = PartitionRepo::new(&conn);
        let records = vec![
            make_partition("p1", "ds-1", 0, "P1"),
            make_partition("p2", "ds-1", 1, "P2"),
            make_partition("p3", "ds-2", 0, "P3"),
        ];
        repo.insert_batch(&records).unwrap();

        assert_eq!(repo.count_by_data_source("ds-1").unwrap(), 2);
        assert_eq!(repo.count_by_data_source("ds-2").unwrap(), 1);
        assert_eq!(repo.count_by_data_source("ds-999").unwrap(), 0);
    }

    #[test]
    fn delete_by_data_source_removes_all() {
        let conn = setup_db();
        let repo = PartitionRepo::new(&conn);
        let records = vec![
            make_partition("p1", "ds-1", 0, "P1"),
            make_partition("p2", "ds-1", 1, "P2"),
        ];
        repo.insert_batch(&records).unwrap();

        let deleted = repo.delete_by_data_source("ds-1").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(repo.count_by_data_source("ds-1").unwrap(), 0);
    }
}
