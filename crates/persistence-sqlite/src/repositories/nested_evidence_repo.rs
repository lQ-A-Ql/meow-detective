use crate::connection::DbResult;
use domain::{DataSourceId, NestedEvidenceLineage};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, PartialEq, Eq)]
pub struct NestedRangeLocator {
    pub parent_id: DataSourceId,
    pub file_id: domain::FileEntryId,
    pub revision: String,
    pub length: u64,
}

pub struct NestedEvidenceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> NestedEvidenceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_range(&self, id: &DataSourceId, locator: &NestedRangeLocator) -> DbResult<()> {
        let length = i64::try_from(locator.length)
            .map_err(|_| crate::connection::DbError::System("nested range is too large".into()))?;
        self.conn.execute(
            "INSERT INTO nested_range_sources(source_id,parent_id,file_id,parent_revision,payload_length)
             VALUES (?1,?2,?3,?4,?5)",
            params![id.0, locator.parent_id.0, locator.file_id.0, locator.revision, length],
        )?;
        Ok(())
    }

    pub fn find_range(&self, id: &DataSourceId) -> DbResult<Option<NestedRangeLocator>> {
        self.conn
            .query_row(
                "SELECT parent_id,file_id,parent_revision,payload_length
             FROM nested_range_sources WHERE source_id=?1",
                [&id.0],
                |row| {
                    Ok(NestedRangeLocator {
                        parent_id: DataSourceId(row.get(0)?),
                        file_id: domain::FileEntryId(row.get(1)?),
                        revision: row.get(2)?,
                        length: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
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
            "INSERT INTO nested_evidence_lineage(parent_data_source_id,nested_file_path,parent_file_id,derived_data_source_id,offset,length,probe_kind)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(parent_data_source_id,nested_file_path) DO UPDATE SET parent_file_id=excluded.parent_file_id, derived_data_source_id=excluded.derived_data_source_id, offset=excluded.offset, length=excluded.length, probe_kind=excluded.probe_kind",
            params![lineage.parent_data_source_id.0, lineage.nested_file_path, lineage.parent_file_id, lineage.derived_data_source_id.as_ref().map(|v| &v.0), i64::try_from(lineage.offset).map_err(|_| crate::connection::DbError::System("nested offset exceeds SQLite range".to_string()))?, i64::try_from(lineage.length).map_err(|_| crate::connection::DbError::System("nested length exceeds SQLite range".to_string()))?, lineage.probe_kind],
        )?;
        Ok(())
    }

    pub fn find_by_parent(&self, parent: &DataSourceId) -> DbResult<Vec<NestedEvidenceLineage>> {
        let mut stmt = self.conn.prepare("SELECT parent_data_source_id,nested_file_path,parent_file_id,derived_data_source_id,offset,length,probe_kind FROM nested_evidence_lineage WHERE parent_data_source_id=?1 ORDER BY nested_file_path")?;
        let rows = stmt.query_map([&parent.0], |row| {
            Ok(NestedEvidenceLineage {
                parent_data_source_id: DataSourceId(row.get(0)?),
                nested_file_path: row.get(1)?,
                parent_file_id: row.get(2)?,
                derived_data_source_id: row.get::<_, Option<String>>(3)?.map(DataSourceId),
                offset: u64::try_from(row.get::<_, i64>(4)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?,
                length: u64::try_from(row.get::<_, i64>(5)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?,
                probe_kind: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
