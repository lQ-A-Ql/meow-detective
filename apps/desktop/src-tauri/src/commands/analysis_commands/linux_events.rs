use tauri::State;
use transport::{
    commands::GetLinuxEvidenceEventsRequest, dto::LinuxEvidenceEventDto, CommandError,
};

use super::support::run_active_case_command;
use crate::state::AppState;

#[tauri::command]
pub async fn get_linux_evidence_events(
    state: State<'_, AppState>,
    mut request: GetLinuxEvidenceEventsRequest,
) -> Result<Vec<LinuxEvidenceEventDto>, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    run_active_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::get_linux_evidence_events(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &request.import_set_id,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
