use std::path::{Path, PathBuf};

use app_services::file_service;
use tauri::State;
use transport::{commands::ExtractFileRequest, CommandError};

use crate::state::AppState;

use super::support::write_file_extract_audit;

/// Extract a file from evidence to a user-selected destination path.
#[tauri::command]
pub async fn extract_file(
    state: State<'_, AppState>,
    request: ExtractFileRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let audit_file_id = request.file_id.clone();
    let audit_destination = request.destination_path.clone();
    let overwrite = request.overwrite;
    let file_id = request.file_id;
    let destination = PathBuf::from(request.destination_path);

    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let outcome = file_service::extract_file_to_destination_for_case(
            &connection,
            &active.case_root,
            &active.meta.id,
            &file_id,
            &destination,
            overwrite,
        )
        .map(|written| format!("Extracted {written} bytes"))
        .map_err(CommandError::from_typed_service_error);

        audit_extract_outcome(
            &app_state,
            &audit_file_id,
            &audit_destination,
            overwrite,
            &outcome,
        );
        outcome
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn audit_extract_outcome(
    state: &AppState,
    file_id: &str,
    destination: &str,
    overwrite: bool,
    outcome: &Result<String, CommandError>,
) {
    let destination_file_name = Path::new(destination)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let details = match outcome {
        Ok(message) => serde_json::json!({
            "status": "ok",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "message": message,
        }),
        Err(error) => serde_json::json!({
            "status": "failed",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "errorCode": error.code,
            "errorCategory": error.category,
        }),
    };
    write_file_extract_audit(state, file_id, details);
}
