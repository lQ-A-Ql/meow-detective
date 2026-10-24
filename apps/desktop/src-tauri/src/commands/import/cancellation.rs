//! Import cancellation command and shared cancellation state helpers.

use tauri::{AppHandle, State};
use transport::{dto::CancellationStateDto, dto::JobCancellationDto, CommandError};

use crate::events::event_bridge;
use crate::state::AppState;

/// Tauri command: Cancel an in-progress import job.
pub async fn cancel_import(
    state: State<'_, AppState>,
    app: AppHandle,
    job_id: String,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let conn =
            app_services::connection::open_existing_case_db(&active.case_root.join("app.db"))
                .map_err(CommandError::from_typed_service_error)?;
        if !app_state.task_manager.is_running(&job_id) {
            return Err(CommandError::not_found("Job"));
        }
        let changed = app_services::job_service::request_cancellation(
            &conn,
            &domain::JobId(job_id.clone()),
            "Cancel requested by user",
        )
        .map_err(CommandError::from_typed_service_error)?;
        if !changed {
            return Ok("Job already settled".into());
        }
        app_state.task_manager.cancel(&job_id);
        event_bridge::emit_job_cancellation(
            &app,
            &job_cancellation_dto(
                &job_id,
                CancellationStateDto::Requested,
                false,
                "Cancel requested by user",
            ),
        );
        Ok("Cancel requested".to_string())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(crate) fn job_cancellation_dto(
    job_id: &str,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) -> JobCancellationDto {
    let now = chrono::Utc::now().to_rfc3339();
    JobCancellationDto {
        job_id: job_id.to_string(),
        requested_at: Some(now.clone()),
        acknowledged_at: matches!(
            state,
            CancellationStateDto::Acknowledged
                | CancellationStateDto::Draining
                | CancellationStateDto::Cancelled
                | CancellationStateDto::TimedOut
        )
        .then_some(now),
        state,
        safe_to_close,
        detail: detail.to_string(),
    }
}

pub(crate) fn is_import_cancelled_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("cancel")
}
