use tauri::State;
use transport::CommandError;

use crate::commands::command_support::{
    require_active_case, write_emulation_edit_audit_log, EmulationAuditEvent,
};
use crate::state::AppState;

#[tauri::command]
pub async fn install_emulation_efi_fallback(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<transport::dto::EmulationEfiFallbackResultDto, CommandError> {
    if session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let result = app_state
            .emulation_registry
            .install_efi_fallback(&session_id)
            .map_err(CommandError::from_typed_service_error)?;
        write_emulation_edit_audit_log(
            &app_state,
            EmulationAuditEvent::EfiFallback,
            &session_id,
            &result.data_source_id,
            serde_json::json!({
                "espPartitionIndex": result.esp_partition_index,
                "strategy": result.strategy.map(|strategy| format!("{strategy:?}")),
                "filesWritten": result.files_written,
                "alreadyPresent": result.already_present,
            }),
        );
        Ok(result)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
