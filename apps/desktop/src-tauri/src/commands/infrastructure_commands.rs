use tauri::State;
use transport::{dto::InfrastructureGraphDto, CommandError};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn get_infrastructure_graph(
    state: State<'_, AppState>,
) -> Result<InfrastructureGraphDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::infrastructure_graph_service::get_infrastructure_graph(
            &connection,
            &domain::CaseId(active.case_id.clone()),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
