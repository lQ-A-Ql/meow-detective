//! Import cancellation command and shared cancellation state helpers.

use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::{AppHandle, State};
use transport::{
    dto::{CancellationStateDto, JobCancellationDto},
    CommandError,
};

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
        if app_state.task_manager.cancel(&job_id) {
            tracing::info!("Cancel requested for job {}", job_id);
            if let Ok(guard) = app_state.active_case.lock() {
                if let Some(active) = guard.as_ref() {
                    match persistence_sqlite::open_or_create(&active.db_path()) {
                        Ok(conn) => {
                            let repo = JobRepo::new(&conn);
                            if let Err(error) = repo.mark_cancelling(
                                &domain::JobId(job_id.clone()),
                                "Cancel requested by user",
                            ) {
                                tracing::warn!(
                                    "Failed to mark job {} as cancelling: {}",
                                    job_id,
                                    error
                                );
                            }
                        }
                        Err(error) => tracing::warn!(
                            "Failed to open case DB while cancelling job {}: {}",
                            job_id,
                            error
                        ),
                    }
                }
            }
            event_bridge::emit_job_cancelled(&app, &job_id, "Cancel requested by user");
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
        } else {
            Err(CommandError::not_found("Job"))
        }
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(crate) fn emit_import_cancellation_state(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) {
    if let Some(app) = app {
        event_bridge::emit_job_cancellation(
            app,
            &job_cancellation_dto(&job_id.0, state, safe_to_close, detail),
        );
    }
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

pub(crate) fn mark_import_cancelling(job_repo: &JobRepo<'_>, job_id: &domain::JobId, detail: &str) {
    if let Err(error) = job_repo.mark_cancelling(job_id, detail) {
        tracing::warn!("Failed to mark job {} as cancelling: {}", job_id.0, error);
    }
}

pub(crate) fn is_import_cancelled_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("cancel")
}
