use tauri::State;
use transport::dto::batch::{BatchJobDto, BatchResumeDto};
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
pub async fn start_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    start_batch_impl(state.inner(), batch_id).await
}

pub(super) async fn start_batch_impl(
    app_state: &AppState,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    validate_batch_id(&batch_id)?;
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::start_batch(&connection, &batch_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn pause_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    pause_batch_impl(state.inner(), batch_id).await
}

pub(super) async fn pause_batch_impl(
    app_state: &AppState,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    validate_batch_id(&batch_id)?;
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::pause_batch(&connection, &batch_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn resume_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    resume_batch_impl(state.inner(), batch_id).await
}

pub(super) async fn resume_batch_impl(
    app_state: &AppState,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    validate_batch_id(&batch_id)?;
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::resume_batch(
            &connection,
            BatchResumeDto {
                batch_id,
                resource_limits: None,
            },
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn cancel_batch(
    state: State<'_, AppState>,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    cancel_batch_impl(state.inner(), batch_id).await
}

pub(super) async fn cancel_batch_impl(
    app_state: &AppState,
    batch_id: String,
) -> Result<BatchJobDto, CommandError> {
    validate_batch_id(&batch_id)?;
    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::batch_service::cancel_batch(&connection, &batch_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
