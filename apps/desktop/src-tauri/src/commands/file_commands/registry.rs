use app_services::file_service;
use tauri::State;
use transport::{dto::RegistryBrowserKeyDto, CommandError};

use crate::state::AppState;

#[tauri::command]
pub async fn browse_registry_key(
    state: State<'_, AppState>,
    file_id: String,
    key_path: String,
) -> Result<RegistryBrowserKeyDto, CommandError> {
    if file_id.trim().is_empty() {
        return Err(CommandError::invalid_input("fileId is required"));
    }
    if key_path.len() > 4096 {
        return Err(CommandError::invalid_input("keyPath is too long"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::browse_registry_key_for_case(
            &app_state.bitlocker_runtime,
            &connection,
            &active.case_root,
            &active.meta.id,
            &file_id,
            &key_path,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
