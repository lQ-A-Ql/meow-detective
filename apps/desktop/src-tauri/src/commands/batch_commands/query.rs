use tauri::State;
use transport::dto::batch::BatchJobDto;
use transport::CommandError;

use crate::commands::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

fn validate_batch_id(batch_id: &str) -> Result<(), CommandError> {
    if batch_id.trim().is_empty() {
        return Err(CommandError::invalid_input("batch_id is required"));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_batch_job(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    get_batch_job_impl(state.inner(), batch_id).await
}

pub(super) async fn get_batch_job_impl(
    app_state: &AppState,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    validate_batch_id(&batch_id)?;
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::get_batch_status(&connection, &batch_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_batch_jobs(state: State<'_, AppState>) -> Result<Vec<BatchJobDto>, CommandError> {
    list_batch_jobs_impl(state.inner()).await
}

pub(super) async fn list_batch_jobs_impl(
    app_state: &AppState,
) -> Result<Vec<BatchJobDto>, CommandError> {
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::list_batch_jobs(&connection, &active.case_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
