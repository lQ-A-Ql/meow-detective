use crate::connection::DbResult;
use crate::repositories::fingerprint_repo::ForensicFingerprintRepo;
use domain::{ForensicFingerprint, ForensicMetadata, ForensicObjectType};
use rusqlite::Connection;

/// Persist FMD records for artifact rows while an analysis staging database is
/// attached as `analysis_stage`.
pub fn persist_analysis_fingerprints(
    conn: &Connection,
    case_id: &str,
    data_source_id: &str,
) -> DbResult<()> {
    let repo = ForensicFingerprintRepo::new(conn);
    if !repo.is_available()? {
        return Ok(());
    }
    let mut statement = conn.prepare(
        "SELECT id, file_id, extractor_id, extractor_version, source_attribution
         FROM analysis_stage.artifact_rows
         ORDER BY id ASC",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let fingerprint = ForensicFingerprint::new(
            ForensicMetadata {
                object_id: row.get(0)?,
                case_id: Some(case_id.to_string()),
                source_id: Some(data_source_id.to_string()),
                object_type: ForensicObjectType::Artifact,
                parent_object_id: row.get(1)?,
                source_locator: row.get(4)?,
                byte_length: None,
                parser_id: row.get(2)?,
                parser_version: row.get(3)?,
            },
            None,
        );
        repo.upsert_in_transaction(&fingerprint)?;
    }
    Ok(())
}

pub fn persist_analysis_fingerprints_for_merge(
    conn: &Connection,
    case_id: &str,
    data_source_id: &str,
) -> rusqlite::Result<()> {
    persist_analysis_fingerprints(conn, case_id, data_source_id)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}
