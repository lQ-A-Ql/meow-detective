use app_services::analysis_service;
use domain::DataSourceId;
use tauri::{AppHandle, State};
use transport::{
    commands::RunPluginExtractionRequest, dto::AnalysisExtractionRunDto, CommandError,
};

use crate::events::event_bridge;
use crate::state::AppState;
use crate::{
    analysis_task_jobs::{self, AnalysisTaskRequest},
    commands::command_support::require_active_case,
};

const PLUGIN_CATEGORIES: &[&str] = &["PluginArtifacts"];

#[tauri::command]
pub async fn run_plugin_extraction(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RunPluginExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);
    let run_id = uuid::Uuid::new_v4().to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.meta.id.0.clone();
        let case_root = active.case_root.clone();
        let worker_case_id = case_id.clone();
        let worker_case_root = case_root.clone();
        let worker_data_source_id = data_source_id.clone();
        let worker_app = app.clone();
        let operation_run_id = run_id.clone();
        let result = analysis_task_jobs::run_with_outcome(
            &app_state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: Some(&data_source_id.0),
                kind: "Plugin extraction",
            },
            move |case_conn, cancel_token| {
                let progress_context = analysis_service::AnalysisExtractionProgressContext::new(
                    &source_runtime,
                    &operation_run_id,
                );
                analysis_service::run_source_analysis_extraction_execution_with_progress_and_cancel(
                    case_conn,
                    &worker_case_root,
                    &domain::CaseId(worker_case_id),
                    &worker_data_source_id,
                    PLUGIN_CATEGORIES,
                    progress_context,
                    cancel_token,
                    |progress| {
                        event_bridge::emit_analysis_extraction_progress(&worker_app, &progress)
                    },
                )
                .map(analysis_task_jobs::AnalysisTaskOutcome::extraction)
                .map_err(CommandError::from_typed_service_error)
            },
        );
        if let Err(error) = &result {
            event_bridge::emit_analysis_extraction_failed(
                &app,
                &run_id,
                &active.meta.id.0,
                &data_source_id.0,
                "PluginArtifacts",
                &error.to_string(),
            );
        }
        result
    })
    .await
    .map_err(CommandError::from_join_error)?
}
