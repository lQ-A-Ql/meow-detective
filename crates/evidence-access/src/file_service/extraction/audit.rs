use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use rusqlite::Connection;

use crate::file_service::FileServiceError;

pub fn record_file_extraction_audit(
    connection: &Connection,
    case_id: Option<&str>,
    file_id: &str,
    details: &serde_json::Value,
) -> Result<(), FileServiceError> {
    let details = serde_json::to_string(details)
        .map_err(|error| FileServiceError::other(format!("serialize extraction audit: {error}")))?;
    AuditRepo::new(connection).log(
        case_id,
        "system",
        &AuditAction::FileExtract,
        Some(file_id),
        &details,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/extraction/audit.rs"]
mod tests;
