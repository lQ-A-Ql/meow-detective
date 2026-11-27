use tauri::State;
use transport::dto::{CaseOverviewSnapshotDto, V3GovernanceSnapshotDto};
use transport::CommandError;

use super::support::run_read_case_command;
use crate::state::AppState;

#[tauri::command]
pub async fn get_v3_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V3GovernanceSnapshotDto, CommandError> {
    run_read_case_command(state.inner().clone(), |connection, active| {
        app_services::v3_governance_service::get_v3_governance_snapshot_for_case_read_only(
            connection,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_case_overview_snapshot(
    state: State<'_, AppState>,
) -> Result<CaseOverviewSnapshotDto, CommandError> {
    run_read_case_command(state.inner().clone(), |connection, active| {
        app_services::v3_governance_service::get_case_overview_snapshot_for_case_read_only(
            connection,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
