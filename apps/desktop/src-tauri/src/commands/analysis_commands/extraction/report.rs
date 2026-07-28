use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{commands::GetAnalysisSourceRequest, CommandError};

use super::super::support::{run_active_case_command, validate_source_request};
use crate::state::AppState;

#[tauri::command]
pub async fn generate_analysis_summary(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<String, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::generate_source_analysis_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &source_runtime,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
