use std::path::PathBuf;

use app_services::file_service;
use tauri::{AppHandle, State};
use transport::{commands::ExtractFileRequest, dto::FileExtractionResultDto, CommandError};

use crate::state::AppState;

use super::extract_progress::{emit_copy_update, emit_preparing, emit_terminal};

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
    let file_id = request.file_id;
    let destination = PathBuf::from(request.destination_path);
    let overwrite = request.overwrite;

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
            file_service::extract_file_for_case_with_audit(
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
