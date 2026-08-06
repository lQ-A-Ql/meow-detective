use tauri::State;
use transport::CommandError;

use crate::commands::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn get_emulation_preflight(
    state: State<'_, AppState>,
    data_source_id: String,
) -> Result<transport::dto::EmulationPreflightDto, CommandError> {
    if data_source_id.trim().is_empty() {
        return Err(CommandError::invalid_input("data source id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let mut preflight = app_services::mount_service::emulation_preflight(
            &connection,
            &active.case_root,
            &active.meta.id,
            &domain::DataSourceId(data_source_id),
        )
        .map_err(CommandError::from_typed_service_error)?;
        preflight.maintenance_tool_available =
            crate::emulation_registry::maintenance_tool_available();
        Ok(preflight)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
