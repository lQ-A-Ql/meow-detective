use app_services::analysis_service;
use transport::{commands::ClassifyFilesRequest, commands::GetAnalysisSourceRequest, CommandError};

use crate::commands::command_support::{
    get_case_connection, require_active_case, ActiveCaseSnapshot,
};
use crate::state::AppState;

pub(super) fn resolve_sample_size(request: &ClassifyFilesRequest) -> Result<u32, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let sample_size = request
        .sample_size
        .unwrap_or(analysis_service::DEFAULT_SAMPLE_SIZE);
    if sample_size == 0 || sample_size > analysis_service::MAX_SAMPLE_SIZE {
        return Err(CommandError::invalid_input(format!(
            "sampleSize must be between 1 and {}",
            analysis_service::MAX_SAMPLE_SIZE
        )));
    }
    Ok(sample_size)
}

pub(super) fn validate_source_request(
    request: &GetAnalysisSourceRequest,
) -> Result<(), CommandError> {
    request.validate().map_err(CommandError::invalid_input)
}

pub(super) async fn run_active_case_command<T, F>(
    app_state: AppState,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, &ActiveCaseSnapshot) -> Result<T, CommandError>
        + Send
        + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_conn = get_case_connection(&app_state)?;
        operation(&case_conn, &active)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
