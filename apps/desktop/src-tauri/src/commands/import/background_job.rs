//! Background import job lifecycle.

use app_services::import_analysis;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use transport::{dto::CancellationStateDto, CommandError};

use crate::events::event_bridge;

use super::{
    cancellation::{is_import_cancelled_message, job_cancellation_dto},
    events::TauriImportEventSink,
    pipeline::{execute_import_job, ImportJobOptions},
};

pub(crate) struct BackgroundImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) import_config: app_services::import_precheck::ImportSourceConfig,
    pub(crate) job_id: domain::JobId,
    pub(crate) max_import_workers: Option<usize>,
    pub(crate) max_analysis_workers: Option<usize>,
    pub(crate) analysis_mode: import_analysis::ImportAnalysisMode,
}

/// Run the background import job in a separate thread.
pub(crate) fn run_background_import_job(
    job: BackgroundImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let conn = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&conn);

    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Import started");
    }

    // Check for cancellation before starting
    if cancel_token.load(Ordering::Relaxed) {
        let msg = "Import cancelled by user";
        if let Err(e) = job_repo.cancel(&job.job_id, msg) {
            tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
        }
        if let Some(app) = app {
            event_bridge::emit_job_cancelled(app, &job.job_id.0, msg);
            event_bridge::emit_job_cancellation(
                app,
                &job_cancellation_dto(&job.job_id.0, CancellationStateDto::Cancelled, true, msg),
            );
        }
        return Ok(());
    }

    let event_sink = app.map(TauriImportEventSink::new);
    let options = ImportJobOptions {
        event_sink: event_sink
            .as_ref()
            .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
        cancel_token: &cancel_token,
        max_import_workers: job.max_import_workers,
        max_analysis_workers: job.max_analysis_workers,
        analysis_mode: job.analysis_mode,
    };
    match execute_import_job(
        &conn,
        &job.case_id,
        &job.case_root,
        job.import_config,
        &job.job_id,
        options,
    ) {
        Ok(message) => {
            job_repo
                .complete(&job.job_id, &message)
                .map_err(CommandError::from_typed_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_completed(app, &job.job_id.0, &message);
            }
            Ok(())
        }
        Err(error) => {
            if is_import_cancelled_message(&error.message) {
                if let Err(e) = job_repo.cancel(&job.job_id, &error.message) {
                    tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
                }
                if let Some(app) = app {
                    event_bridge::emit_job_cancelled(app, &job.job_id.0, &error.message);
                    event_bridge::emit_job_cancellation(
                        app,
                        &job_cancellation_dto(
                            &job.job_id.0,
                            CancellationStateDto::Cancelled,
                            true,
                            &error.message,
                        ),
                    );
                }
                Ok(())
            } else {
                if let Err(e) = job_repo.fail(&job.job_id, &error.message) {
                    tracing::error!("Failed to mark job {} as failed: {}", job.job_id.0, e);
                }
                if let Some(app) = app {
                    event_bridge::emit_job_failed(app, &job.job_id.0, &error.message);
                }
                Err(error)
            }
        }
    }
}
