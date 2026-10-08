use app_services::file_service;
use tauri::State;
use transport::{dto::NtfsTechnicalFileDto, CommandError};

use crate::state::AppState;

/// Load the bounded NTFS technical inspector for an active-case file.
#[tauri::command]
pub async fn inspect_ntfs_file(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<NtfsTechnicalFileDto, CommandError> {
    if file_id.trim().is_empty() {
        return Err(CommandError::invalid_input("fileId is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::inspect_ntfs_file_for_case(
            &app_state.bitlocker_runtime,
            &connection,
            &active.case_root,
            &active.meta.id,
            &file_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
