use tauri::{AppHandle, State};
use transport::{
    commands::{GetLinuxEvidenceSetSummaryRequest, ListLinuxEvidenceSetsRequest},
    dto::{KubernetesAnalysisRunDto, LinuxEvidenceSetListItemDto, LinuxEvidenceSetSummaryDto},
    CommandError,
};

use super::support::run_read_case_command;
use crate::analysis_task_jobs::{self, AnalysisTaskRequest};
use crate::commands::command_support::require_active_case;
use crate::state::AppState;

#[tauri::command]
pub async fn get_linux_evidence_set_summary(
    state: State<'_, AppState>,
    request: GetLinuxEvidenceSetSummaryRequest,
) -> Result<LinuxEvidenceSetSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    run_read_case_command(app_state, move |case_conn, active| {
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
    run_read_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::list_linux_evidence_sets(case_conn, &active.meta.id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn run_kubernetes_cluster_analysis(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GetLinuxEvidenceSetSummaryRequest,
) -> Result<KubernetesAnalysisRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.meta.id.0.clone();
        let case_root = active.case_root.clone();
        let import_set_id = request.import_set_id;
        let worker_case_id = case_id.clone();
        let worker_case_root = case_root.clone();
        let worker_import_set_id = import_set_id.clone();
        analysis_task_jobs::run(
            &app_state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: None,
                kind: "Kubernetes cluster analysis",
            },
            move |case_conn, cancel_token| {
                app_services::cluster_service::run_kubernetes_cluster_analysis_with_cancel(
                    case_conn,
                    &worker_case_root,
                    &domain::CaseId(worker_case_id),
                    &worker_import_set_id,
                    &cancel_token,
                )
                .map_err(CommandError::from_typed_service_error)
            },
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}
