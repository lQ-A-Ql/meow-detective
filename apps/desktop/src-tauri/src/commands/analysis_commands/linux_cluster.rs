use tauri::State;
use transport::{
    commands::{GetLinuxEvidenceSetSummaryRequest, ListLinuxEvidenceSetsRequest},
    dto::{KubernetesAnalysisRunDto, LinuxEvidenceSetListItemDto, LinuxEvidenceSetSummaryDto},
    CommandError,
};

use super::support::run_active_case_command;
use crate::state::AppState;

#[tauri::command]
pub async fn get_linux_evidence_set_summary(
    state: State<'_, AppState>,
    request: GetLinuxEvidenceSetSummaryRequest,
) -> Result<LinuxEvidenceSetSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    run_active_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::get_linux_evidence_set_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &request.import_set_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn list_linux_evidence_sets(
    state: State<'_, AppState>,
    _request: ListLinuxEvidenceSetsRequest,
) -> Result<Vec<LinuxEvidenceSetListItemDto>, CommandError> {
    let app_state = state.inner().clone();
    run_active_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::list_linux_evidence_sets(case_conn, &active.meta.id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn run_kubernetes_cluster_analysis(
    state: State<'_, AppState>,
    request: GetLinuxEvidenceSetSummaryRequest,
) -> Result<KubernetesAnalysisRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    run_active_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::run_kubernetes_cluster_analysis(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &request.import_set_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
