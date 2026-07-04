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
                unlock_hint TEXT,
                lvm_vg_uuid TEXT,
                lvm_vg_name TEXT,
                lvm_lv_uuid TEXT,
                lvm_lv_name TEXT,
                lvm_pv_offsets_json TEXT,
                lvm_pv_sources_json TEXT
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
            lvm_vg_uuid: None,
            lvm_vg_name: None,
            lvm_lv_uuid: None,
            lvm_lv_name: None,
            lvm_pv_offsets_json: None,
            lvm_pv_sources_json: None,
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

    #[test]
    fn lvm_identity_round_trips() {
        let conn = setup_db();
        let repo = PartitionRepo::new(&conn);
        let mut record = make_partition("p-lvm", "ds-lvm", 2, "vg/root");
        record.filesystem = Some("XFS".to_string());
        record.lvm_vg_uuid = Some("vg-uuid".to_string());
        record.lvm_vg_name = Some("vg".to_string());
        record.lvm_lv_uuid = Some("lv-uuid".to_string());
        record.lvm_lv_name = Some("root".to_string());
        record.lvm_pv_offsets_json = Some("[1048576,2097152]".to_string());
        record.lvm_pv_sources_json = Some(
            r#"[{"sourcePath":"disk1.E01","offset":1048576,"pvUuid":"pv-uuid-1","pvName":"pv0"},{"sourcePath":"disk2.E01","offset":2097152,"pvUuid":"pv-uuid-2","pvName":"pv1"}]"#
                .to_string(),
        );

        repo.insert_batch(&[record]).unwrap();

        let found = repo.find_by_data_source("ds-lvm").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].lvm_vg_uuid.as_deref(), Some("vg-uuid"));
        assert_eq!(found[0].lvm_vg_name.as_deref(), Some("vg"));
        assert_eq!(found[0].lvm_lv_uuid.as_deref(), Some("lv-uuid"));
        assert_eq!(found[0].lvm_lv_name.as_deref(), Some("root"));
        assert_eq!(
            found[0].lvm_pv_offsets_json.as_deref(),
            Some("[1048576,2097152]")
        );
        assert_eq!(
            found[0].lvm_pv_sources_json.as_deref(),
            Some(
                r#"[{"sourcePath":"disk1.E01","offset":1048576,"pvUuid":"pv-uuid-1","pvName":"pv0"},{"sourcePath":"disk2.E01","offset":2097152,"pvUuid":"pv-uuid-2","pvName":"pv1"}]"#
            )
        );
    }
}
