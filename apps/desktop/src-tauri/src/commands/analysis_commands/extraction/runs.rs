use app_services::analysis_service;
use domain::DataSourceId;
use tauri::{AppHandle, State};
use transport::{
    commands::{RunAnalysisExtractionRequest, RunEvidenceClassificationRequest},
    dto::{AnalysisExtractionRunDto, EvidenceClassificationSummaryDto},
    CommandError,
};

use super::super::support::run_active_case_command;
use crate::events::event_bridge;
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
    app: AppHandle,
    state: State<'_, AppState>,
    request: RunAnalysisExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;
    let run_id = uuid::Uuid::new_v4().to_string();

    run_active_case_command(app_state, move |case_conn, active| {
        let categories = requested_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let result = analysis_service::run_source_analysis_extraction_with_progress(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &categories,
            &run_id,
            |progress| event_bridge::emit_analysis_extraction_progress(&app, &progress),
        );
        if let Err(error) = &result {
            event_bridge::emit_analysis_extraction_failed(
                &app,
                &run_id,
                &active.meta.id.0,
                &data_source_id.0,
                requested_categories
                    .first()
                    .map(String::as_str)
                    .unwrap_or("analysis"),
                &error.to_string(),
            );
        }
        result.map_err(CommandError::from_typed_service_error)
    })
    .await
}
