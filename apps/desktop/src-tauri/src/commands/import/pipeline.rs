//! Import execution pipeline.
//!
//! Keeps the filesystem enumeration and post-import analysis workflow. Tauri
//! command wrappers remain here to preserve the existing handler paths.

use tauri::{AppHandle, State};
use transport::{commands::ImportDataSourceRequest, CommandError};

use crate::state::AppState;

/// Tauri command: Import a data source into the current case.
#[tauri::command]
pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    super::schedule::import_data_source(state, app, request).await
}

/// Tauri command: Cancel an in-progress import job.
#[tauri::command]
pub async fn cancel_import(
    state: State<'_, AppState>,
    app: AppHandle,
    job_id: String,
) -> Result<String, CommandError> {
    super::cancellation::cancel_import(state, app, job_id).await
}

// Re-export the import pipeline API used by sibling command modules.
pub use app_services::import_pipeline::{execute_import_job, ImportJobOptions};

#[cfg(test)]
mod tests {
    use transport::dto::CancellationStateDto;

    use super::super::cancellation::job_cancellation_dto;

    #[test]
    fn job_cancellation_dto_maps_requested_and_draining_states() {
        let requested = job_cancellation_dto(
            "job-cancel-1",
            CancellationStateDto::Requested,
            false,
            "Cancel requested by user",
        );
        assert_eq!(requested.job_id, "job-cancel-1");
        assert_eq!(requested.state, CancellationStateDto::Requested);
        assert!(!requested.safe_to_close);
        assert!(requested.requested_at.is_some());
        assert!(requested.acknowledged_at.is_none());

        let draining = job_cancellation_dto(
            "job-cancel-1",
            CancellationStateDto::Draining,
            false,
            "Cancellation acknowledged; draining workers",
        );
        assert_eq!(draining.state, CancellationStateDto::Draining);
        assert!(!draining.safe_to_close);
        assert!(draining.requested_at.is_some());
        assert!(draining.acknowledged_at.is_some());
    }
}
