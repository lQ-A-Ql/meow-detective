use crate::connection::DbResult;
use domain::{DataSourceId, NestedEvidenceLineage};
use rusqlite::{params, Connection};

pub struct NestedEvidenceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> NestedEvidenceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, lineage: &NestedEvidenceLineage) -> DbResult<()> {
        if lineage.nested_file_path.trim().is_empty()
            || lineage.probe_kind.trim().is_empty()
            || lineage.length == 0
        {
            return Err(crate::connection::DbError::System(
                "nested evidence lineage is incomplete".to_string(),
            ));
        }
        self.conn.execute(
            "INSERT INTO nested_evidence_lineage(parent_data_source_id,nested_file_path,derived_data_source_id,offset,length,probe_kind)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(parent_data_source_id,nested_file_path) DO UPDATE SET derived_data_source_id=excluded.derived_data_source_id, offset=excluded.offset, length=excluded.length, probe_kind=excluded.probe_kind",
            params![lineage.parent_data_source_id.0, lineage.nested_file_path, lineage.derived_data_source_id.as_ref().map(|v| &v.0), i64::try_from(lineage.offset).map_err(|_| crate::connection::DbError::System("nested offset exceeds SQLite range".to_string()))?, i64::try_from(lineage.length).map_err(|_| crate::connection::DbError::System("nested length exceeds SQLite range".to_string()))?, lineage.probe_kind],
        )?;
        Ok(())
    }

    pub fn find_by_parent(&self, parent: &DataSourceId) -> DbResult<Vec<NestedEvidenceLineage>> {
        let mut stmt = self.conn.prepare("SELECT parent_data_source_id,nested_file_path,derived_data_source_id,offset,length,probe_kind FROM nested_evidence_lineage WHERE parent_data_source_id=?1 ORDER BY nested_file_path")?;
        let rows = stmt.query_map([&parent.0], |row| {
            Ok(NestedEvidenceLineage {
                parent_data_source_id: DataSourceId(row.get(0)?),
                nested_file_path: row.get(1)?,
                derived_data_source_id: row.get::<_, Option<String>>(2)?.map(DataSourceId),
                offset: u64::try_from(row.get::<_, i64>(3)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?,
                length: u64::try_from(row.get::<_, i64>(4)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?,
                probe_kind: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
