use tauri::State;
use transport::dto::{CorrelationSnapshotDto, V2GovernanceSnapshotDto, V3GovernanceSnapshotDto};
use transport::CommandError;

use super::support::run_active_case_command;
use crate::state::AppState;

#[tauri::command]
pub async fn get_v2_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V2GovernanceSnapshotDto, CommandError> {
    run_active_case_command(state.inner().clone(), |connection, active| {
        app_services::v2_governance_service::get_v2_governance_snapshot_for_case(
            connection,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_v3_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V3GovernanceSnapshotDto, CommandError> {
    run_active_case_command(state.inner().clone(), |connection, active| {
        app_services::v3_governance_service::get_v3_governance_snapshot_for_case(
            connection,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_correlation_snapshot(
    state: State<'_, AppState>,
) -> Result<CorrelationSnapshotDto, CommandError> {
    run_active_case_command(state.inner().clone(), |connection, active| {
        app_services::correlation::get_correlation_snapshot_for_case(
            connection,
            &active.case_root,
            &domain::CaseId(active.case_id.clone()),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
