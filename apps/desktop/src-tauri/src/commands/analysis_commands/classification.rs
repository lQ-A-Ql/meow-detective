use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::ClassifyFilesRequest,
    dto::{AnalysisFileClassificationDto, FileClassificationBoardDto},
    CommandError,
};

use super::support::{resolve_sample_size, run_active_case_command};
use crate::state::AppState;

#[tauri::command]
pub async fn classify_files(
    state: State<'_, AppState>,
    request: ClassifyFilesRequest,
) -> Result<Vec<AnalysisFileClassificationDto>, CommandError> {
    let sample_size = resolve_sample_size(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::classify_source_files(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            sample_size,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_file_classification_board(
    state: State<'_, AppState>,
    request: ClassifyFilesRequest,
) -> Result<FileClassificationBoardDto, CommandError> {
    let magic_read_limit = resolve_sample_size(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_file_classification_board(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            magic_read_limit,
            &source_runtime,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
