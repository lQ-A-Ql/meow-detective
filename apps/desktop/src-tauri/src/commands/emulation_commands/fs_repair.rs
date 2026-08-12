use tauri::State;
use transport::CommandError;

use crate::commands::command_support::{
    get_case_connection, require_active_case, write_emulation_edit_audit_log, EmulationAuditEvent,
};
use crate::state::AppState;

#[tauri::command]
pub async fn repair_emulation_fs_journals(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<transport::dto::EmulationFsRepairResultDto, CommandError> {
    if session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let result = app_state
            .emulation_registry
            .repair_fs_journals(
                &crate::emulation_registry::BypassCaseRef {
                    case_conn: &connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
                &session_id,
            )
            .map_err(CommandError::from_typed_service_error)?;
        write_emulation_edit_audit_log(
            &app_state,
            EmulationAuditEvent::FsRepair,
            &session_id,
            &result.data_source_id,
            serde_json::json!({
                "items": result.items.iter().map(|item| serde_json::json!({
                    "partitionIndex": item.partition_index,
                    "state": format!("{:?}", item.state),
                    "repaired": item.repaired,
                    "logBytes": item.log_bytes,
                })).collect::<Vec<_>>(),
            }),
        );
        Ok(result)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
