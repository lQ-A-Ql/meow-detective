use persistence_fjall::PersistMode;
use rusqlite::Connection;

use crate::storage_runtime::ReadWriteSourceStorageHandle;
use transport::CommandError;

pub(super) fn mirror_nested_lineage(
    case_connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let mut statement = case_connection
        .prepare(
            "SELECT parent_data_source_id, nested_file_path, parent_file_id,
                    derived_data_source_id, offset, length, probe_kind
             FROM nested_evidence_lineage
             WHERE parent_data_source_id = ?1
             ORDER BY nested_file_path ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([source_id], |row| {
            let lineage = domain::NestedEvidenceLineage {
                parent_data_source_id: domain::DataSourceId(row.get(0)?),
                nested_file_path: row.get(1)?,
                parent_file_id: row.get(2)?,
                derived_data_source_id: row.get::<_, Option<String>>(3)?.map(domain::DataSourceId),
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
            };
            Ok(crate::nested_evidence_service::lineage_parser_record(
                &lineage,
            ))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        source
            .parser_upsert(
                &row.map_err(|error| CommandError::internal(error.to_string()))?,
                PersistMode::Buffer,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}
