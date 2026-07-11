use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::{RunAnalysisExtractionRequest, RunEvidenceClassificationRequest},
    dto::{AnalysisExtractionRunDto, EvidenceClassificationSummaryDto},
    CommandError,
};

use super::super::support::run_active_case_command;
use crate::state::AppState;

#[tauri::command]
pub async fn run_evidence_classification(
    state: State<'_, AppState>,
    request: RunEvidenceClassificationRequest,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;

    run_active_case_command(app_state, move |case_conn, active| {
        let categories = requested_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        analysis_service::run_source_evidence_scan(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &categories,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn run_analysis_extraction(
    state: State<'_, AppState>,
    request: RunAnalysisExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;

    run_active_case_command(app_state, move |case_conn, active| {
        let categories = requested_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        analysis_service::run_source_analysis_extraction(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &categories,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
