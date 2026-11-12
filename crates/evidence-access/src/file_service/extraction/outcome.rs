//! Audited extraction orchestration: run the case file extraction, record the
//! outcome in the audit log, and merge both into the published result.

use std::path::Path;
use std::sync::Arc;

use transport::{dto::FileExtractionResultDto, CommandError};

use super::audit::record_file_extraction_audit;
use super::{
    extract_file_to_destination_for_case_with_bitlocker_and_progress, CaseFileExtractionRequest,
    FileExtractionProgressCallback,
};
use crate::bitlocker_runtime::BitLockerUnlockRegistry;

const AUDIT_PERSISTENCE_WARNING: &str =
    "The file was extracted, but its audit record could not be persisted. Verify the destination before continuing.";

/// Extract a case file to a user-selected destination and persist the audit
/// record for the attempt. A completed extraction whose audit record cannot
/// be persisted is still returned as a success carrying a warning; a failed
/// extraction keeps its original error whether or not the failure audit
/// could be written.
pub fn extract_file_for_case_with_audit(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    request: CaseFileExtractionRequest<'_>,
    report_progress: FileExtractionProgressCallback<'_>,
) -> Result<FileExtractionResultDto, CommandError> {
    let case_conn = request.case_conn;
    let case_id = request.case_id;
    let file_id = request.file_id;
    let destination_path = request.destination_path;
    let overwrite = request.overwrite;
    let outcome = extract_file_to_destination_for_case_with_bitlocker_and_progress(
        bitlocker_runtime,
        request,
        report_progress,
    )
    .map_err(CommandError::from_typed_service_error);
    let audit_result = audit_extraction_outcome(
        case_conn,
        Some(&case_id.0),
        file_id,
        destination_path,
        overwrite,
        &outcome,
    );
    resolve_extraction_with_audit(outcome, audit_result)
}

/// Build the outcome-specific audit details and persist them.
fn audit_extraction_outcome(
    connection: &rusqlite::Connection,
    case_id: Option<&str>,
    file_id: &str,
    destination: &Path,
    overwrite: bool,
    outcome: &Result<FileExtractionResultDto, CommandError>,
) -> Result<(), CommandError> {
    let destination_file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let details = match outcome {
        Ok(result) => serde_json::json!({
            "status": "ok",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "bytesWritten": result.bytes_written,
            "sourceSize": result.source_size,
            "sha256": result.sha256,
            "sizeVerified": result.size_verified,
        }),
        Err(error) => serde_json::json!({
            "status": "failed",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "errorCode": error.code,
            "errorCategory": error.category,
        }),
    };
    record_file_extraction_audit(connection, case_id, file_id, &details)
        .map_err(CommandError::from_typed_service_error)
}

/// Merge the extraction outcome with its audit result.
fn resolve_extraction_with_audit(
    outcome: Result<FileExtractionResultDto, CommandError>,
    audit_result: Result<(), CommandError>,
) -> Result<FileExtractionResultDto, CommandError> {
    match (outcome, audit_result) {
        (Ok(mut result), Ok(())) => {
            result.audit_persisted = true;
            result.warning = None;
            Ok(result)
        }
        (Ok(mut result), Err(error)) => {
            tracing::error!(
                error_code = %error.code,
                "File was extracted but its audit record could not be persisted"
            );
            result.audit_persisted = false;
            result.warning = Some(AUDIT_PERSISTENCE_WARNING.to_string());
            Ok(result)
        }
        (Err(error), Ok(())) => Err(error),
        (Err(operation_error), Err(audit_error)) => {
            tracing::error!(
                error_code = %audit_error.code,
                "Failed extraction could not be recorded in the audit log"
            );
            Err(operation_error)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/extraction/outcome.rs"]
mod tests;
