use std::path::{Path, PathBuf};

use app_services::file_service;
use tauri::{AppHandle, State};
use transport::{commands::ExtractFileRequest, dto::FileExtractionResultDto, CommandError};

use crate::state::AppState;

use super::extract_progress::{emit_copy_update, emit_preparing, emit_terminal};
use super::support::persist_file_extract_audit;

const AUDIT_PERSISTENCE_WARNING: &str =
    "The file was extracted, but its audit record could not be persisted. Verify the destination before continuing.";

/// Extract a file from evidence to a user-selected destination path.
#[tauri::command]
pub async fn extract_file(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExtractFileRequest,
) -> Result<FileExtractionResultDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    emit_preparing(&app, &request.operation_id, &request.file_id);
    let app_state = state.inner().clone();
    let operation_id = request.operation_id;
    let audit_file_id = request.file_id.clone();
    let audit_destination = request.destination_path.clone();
    let overwrite = request.overwrite;
    let file_id = request.file_id;
    let destination = PathBuf::from(request.destination_path);

    tauri::async_runtime::spawn_blocking(move || {
        let mut last_bytes_written = 0;
        let mut last_total_bytes = None;
        let result = (|| {
            let connection = crate::commands::command_support::get_case_connection(&app_state)?;
            let active = crate::commands::command_support::require_active_case(&app_state)?;
            let mut report_progress = |update: file_service::FileExtractionProgressUpdate| {
                last_bytes_written = update.bytes_written;
                last_total_bytes = update.total_bytes;
                emit_copy_update(&app, &operation_id, &file_id, update);
            };
            let outcome =
                file_service::extract_file_to_destination_for_case_with_bitlocker_and_progress(
                    &app_state.bitlocker_runtime,
                    file_service::CaseFileExtractionRequest {
                        case_conn: &connection,
                        case_root: &active.case_root,
                        case_id: &active.meta.id,
                        file_id: &file_id,
                        destination_path: &destination,
                        overwrite,
                    },
                    &mut report_progress,
                )
                .map_err(CommandError::from_typed_service_error);

            let audit_result = audit_extract_outcome(
                &connection,
                &active.meta.id.0,
                &audit_file_id,
                &audit_destination,
                overwrite,
                &outcome,
            );
            resolve_extract_and_audit(outcome, audit_result)
        })();

        emit_terminal(
            &app,
            &operation_id,
            &file_id,
            &result,
            last_bytes_written,
            last_total_bytes,
        );
        result
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn audit_extract_outcome(
    connection: &rusqlite::Connection,
    case_id: &str,
    file_id: &str,
    destination: &str,
    overwrite: bool,
    outcome: &Result<FileExtractionResultDto, CommandError>,
) -> Result<(), CommandError> {
    let destination_file_name = Path::new(destination)
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
    persist_file_extract_audit(connection, Some(case_id), file_id, details)
}

pub(super) fn resolve_extract_and_audit(
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
