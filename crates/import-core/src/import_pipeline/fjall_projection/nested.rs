use persistence_source::fjall::PersistMode;
use rusqlite::Connection;

use source_kernel::storage_runtime::ReadWriteSourceStorageHandle;
use transport::CommandError;

const BATCH_SIZE: usize = 512;

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
            Ok(evidence_access::nested_lineage::lineage_parser_record(
                &lineage,
            ))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for row in rows {
        batch.push(row.map_err(|error| CommandError::internal(error.to_string()))?);
        if batch.len() == BATCH_SIZE {
            source
                .parser_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .parser_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}
