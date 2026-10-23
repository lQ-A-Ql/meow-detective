use app_services::analysis_service;
use domain::DataSourceId;
use tauri::{AppHandle, State};
use transport::{
    commands::{RunAnalysisExtractionRequest, RunEvidenceClassificationRequest},
    dto::{AnalysisExtractionRunDto, EvidenceClassificationSummaryDto},
    CommandError,
};

use crate::events::event_bridge;
use crate::state::AppState;
use crate::{
    analysis_task_jobs::{self, AnalysisTaskRequest},
    commands::command_support::require_active_case,
};

#[tauri::command]
pub async fn run_evidence_classification(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RunEvidenceClassificationRequest,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.meta.id.0.clone();
        let case_root = active.case_root.clone();
        let data_source_id = data_source_id.clone();
        let worker_case_id = case_id.clone();
        let worker_case_root = case_root.clone();
        let worker_data_source_id = data_source_id.clone();
        analysis_task_jobs::run(
            &app_state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: Some(&data_source_id.0),
                kind: "Evidence classification",
            },
            move |case_conn, cancel_token| {
                let category_refs = requested_categories
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                analysis_service::run_source_evidence_scan_with_cancel(
                    case_conn,
                    &worker_case_root,
                    &domain::CaseId(worker_case_id),
                    &worker_data_source_id,
                    &category_refs,
                    &source_runtime,
                    &cancel_token,
                )
                .map_err(CommandError::from_typed_service_error)
            },
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn run_analysis_extraction(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RunAnalysisExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;
    let browser_password = request.browser_password;
    let run_id = uuid::Uuid::new_v4().to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.meta.id.0.clone();
        let case_root = active.case_root.clone();
        let data_source_id = data_source_id.clone();
        let worker_case_id = case_id.clone();
        let worker_case_root = case_root.clone();
        let worker_data_source_id = data_source_id.clone();
        let worker_app = app.clone();
        let operation_run_id = run_id.clone();
        let worker_categories = requested_categories.clone();
        let result = analysis_task_jobs::run(
            &app_state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: Some(&data_source_id.0),
                kind: "Analysis extraction",
            },
            move |case_conn, cancel_token| {
                let category_refs = worker_categories
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let mut progress_context = analysis_service::AnalysisExtractionProgressContext::new(
                    &source_runtime,
                    &operation_run_id,
                );
                if let Some(password) = browser_password {
                    progress_context = progress_context.with_browser_password(password);
                }
                analysis_service::run_source_analysis_extraction_with_progress_and_cancel(
                    case_conn,
                    &worker_case_root,
                    &domain::CaseId(worker_case_id),
                    &worker_data_source_id,
                    &category_refs,
                    progress_context,
                    &cancel_token,
                    |progress| {
                        event_bridge::emit_analysis_extraction_progress(&worker_app, &progress)
                    },
                )
                .map_err(CommandError::from_typed_service_error)
            },
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
        result
    })
    .await
    .map_err(CommandError::from_join_error)?
}
