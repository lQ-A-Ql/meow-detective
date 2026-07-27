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
    pub lvm_vg_uuid: Option<String>,
    pub lvm_vg_name: Option<String>,
    pub lvm_lv_uuid: Option<String>,
    pub lvm_lv_name: Option<String>,
    pub lvm_pv_offsets_json: Option<String>,
    pub lvm_pv_sources_json: Option<String>,
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
                 (id, data_source_id, partition_index, name, kind_label, status, type_guid,
                  offset, length, filesystem, unlock_hint, lvm_vg_uuid, lvm_vg_name,
                  lvm_lv_uuid, lvm_lv_name, lvm_pv_offsets_json, lvm_pv_sources_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
                    record.lvm_vg_uuid,
                    record.lvm_vg_name,
                    record.lvm_lv_uuid,
                    record.lvm_lv_name,
                    record.lvm_pv_offsets_json,
                    record.lvm_pv_sources_json,
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
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid,
                    offset, length, filesystem, unlock_hint, lvm_vg_uuid, lvm_vg_name,
                    lvm_lv_uuid, lvm_lv_name, lvm_pv_offsets_json, lvm_pv_sources_json
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
                lvm_vg_uuid: row.get(11)?,
                lvm_vg_name: row.get(12)?,
                lvm_lv_uuid: row.get(13)?,
                lvm_lv_name: row.get(14)?,
                lvm_pv_offsets_json: row.get(15)?,
                lvm_pv_sources_json: row.get(16)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn find_by_data_source_and_index(
        &self,
        data_source_id: &str,
        partition_index: usize,
    ) -> DbResult<Option<DataSourcePartitionRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, data_source_id, partition_index, name, kind_label, status, type_guid,
                    offset, length, filesystem, unlock_hint, lvm_vg_uuid, lvm_vg_name,
                    lvm_lv_uuid, lvm_lv_name, lvm_pv_offsets_json, lvm_pv_sources_json
             FROM data_source_partitions
             WHERE data_source_id = ?1 AND partition_index = ?2",
        )?;
        let mut rows = stmt.query(params![data_source_id, partition_index as u64])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(DataSourcePartitionRecord {
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
            lvm_vg_uuid: row.get(11)?,
            lvm_vg_name: row.get(12)?,
            lvm_lv_uuid: row.get(13)?,
            lvm_lv_name: row.get(14)?,
            lvm_pv_offsets_json: row.get(15)?,
            lvm_pv_sources_json: row.get(16)?,
        }))
    }

    pub fn mark_bitlocker_catalog_ready(
        &self,
        data_source_id: &str,
        partition_index: u32,
        filesystem: &str,
    ) -> DbResult<usize> {
        self.conn
            .execute(
                "UPDATE data_source_partitions
                 SET status = 'ready', filesystem = ?1, unlock_hint = NULL
                 WHERE data_source_id = ?2 AND partition_index = ?3",
                params![filesystem, data_source_id, partition_index],
            )
            .map_err(Into::into)
    }

    /// 批量插入分区记录
    pub fn insert_batch(&self, records: &[DataSourcePartitionRecord]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO data_source_partitions
                 (id, data_source_id, partition_index, name, kind_label, status, type_guid,
                  offset, length, filesystem, unlock_hint, lvm_vg_uuid, lvm_vg_name,
                  lvm_lv_uuid, lvm_lv_name, lvm_pv_offsets_json, lvm_pv_sources_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
                    record.lvm_vg_uuid,
                    record.lvm_vg_name,
                    record.lvm_lv_uuid,
                    record.lvm_lv_name,
                    record.lvm_pv_offsets_json,
                    record.lvm_pv_sources_json,
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
#[path = "../../tests/unit/repositories/partition_repo.rs"]
mod tests;
