use tauri::State;
use transport::CommandError;

use crate::commands::command_support::{get_case_read_connection, require_active_case};
use crate::commands::emulation_commands::worker::run_emulation_blocking;
use crate::state::AppState;

#[tauri::command]
pub async fn get_emulation_linux_accounts(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<Vec<transport::dto::EmulationLinuxAccountDto>, CommandError> {
    if data_source_id.trim().is_empty() {
        return Err(CommandError::invalid_input("data source id is required"));
    }
    let app_state = state.inner().clone();
    run_emulation_blocking("list-linux-accounts", move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_read_connection(&app_state)?;
        app_services::emulation_linux_bypass::list_linux_accounts(
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: &connection,
                case_root: &active.case_root,
                case_id: &active.meta.id,
                data_source_id: &domain::DataSourceId(data_source_id),
            },
            partition_index,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn inspect_emulation_linux_bypass(
    state: State<'_, AppState>,
    request: transport::dto::EmulationLinuxBypassInspectRequestDto,
) -> Result<transport::dto::EmulationLinuxBypassInspectionDto, CommandError> {
    if request.session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    let app_state = state.inner().clone();
    run_emulation_blocking("inspect-linux-bypass", move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_read_connection(&app_state)?;
        app_state
            .emulation_registry
            .inspect_linux_bypass(
                &crate::emulation_registry::BypassCaseRef {
                    case_conn: &connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
                &request.session_id,
                request.partition_index,
            )
            .map_err(CommandError::from_typed_service_error)
    })
    .await
}
