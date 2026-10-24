use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;
use transport::CommandError;

use super::{super::events::TauriImportEventSink, types::BackgroundImportJob};
use crate::events::event_bridge;
use crate::state::TaskManager;

pub(crate) fn run_background_import_job(
    job: BackgroundImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
    task_manager: Arc<TaskManager>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_existing_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let event_sink = app.map(TauriImportEventSink::new);
    match app_services::import_job_coordinator::run_single_import(
        app_services::import_job_coordinator::SingleImportRequest {
            connection: &connection,
            case_id: &job.case_id,
            case_root: &job.case_root,
            job_id: &job.job_id,
            import_config: job.import_config,
            max_import_workers: job.max_import_workers,
            max_analysis_workers: job.max_analysis_workers,
            analysis_mode: job.analysis_mode,
            cancel_token: &cancel_token,
            event_sink: event_sink
                .as_ref()
                .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
        },
    ) {
        Ok(app_services::import_job_coordinator::SingleImportOutcome::Completed(message)) => {
            if let Some(app) = app {
                event_bridge::emit_job_completed(app, &job.job_id.0, &message);
                crate::case_projection_jobs::schedule(
                    &task_manager,
                    app,
                    &job.case_id.0,
                    &job.case_root,
                )?;
            }
            Ok(())
        }
        Ok(app_services::import_job_coordinator::SingleImportOutcome::Cancelled(message)) => {
            if let Some(app) = app {
                event_bridge::emit_job_cancelled(app, &job.job_id.0, &message);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(app) = app {
                event_bridge::emit_job_failed(app, &job.job_id.0, &error.message);
            }
            Err(error)
        }
    }
}
