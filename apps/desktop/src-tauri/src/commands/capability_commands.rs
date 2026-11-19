use crate::commands::command_support::{get_case_read_connection, require_active_case};
use crate::state::AppState;
use app_services::capability_service;
use tauri::State;
use transport::dto::CapabilityMatrixDto;
use transport::CommandError;

/// Read the derived per-source capability matrix for the active case.
///
/// L0 is read-only: the matrix is derived from persisted markers; no import
/// or post-processing state changes.
#[tauri::command]
pub async fn get_source_capabilities(
    state: State<'_, AppState>,
) -> Result<CapabilityMatrixDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_read_connection(&app_state)?;
        capability_service::derive_capability_matrix(&conn, &active.case_root, &active.meta.id)
            .map_err(|error| CommandError::internal(error.to_string()))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
