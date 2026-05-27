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
}
