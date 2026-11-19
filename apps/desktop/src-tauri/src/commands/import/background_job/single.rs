use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;
use transport::CommandError;

use super::super::events::TauriImportEventSink;
use super::types::BackgroundImportJob;
use crate::events::event_bridge;
use crate::state::TaskManager;

pub(crate) fn run_background_import_job(
    job: BackgroundImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
    task_manager: Arc<TaskManager>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_existing_case_db(&job.db_path)
        .map_err(app_services::connection::command_error_from_db)?;
    let event_sink = app.map(TauriImportEventSink::new);
    // Captured before `import_config` moves into the coordinator request; the
    // Completed branch resolves the new source row by this path.
    let source_path = job.import_config.source_path.clone();
    // L5 rollback switch: `capabilityLifecycle.enabled = false` selects Inline
    // (timeline/artifacts run inside the job) and suppresses shell enqueue.
    // Read once at job start; a change never flips a running import mid-way.
    let mode = app_services::import_job_coordinator::SingleImportMode::from_enabled(
        crate::commands::settings_commands::read_capability_lifecycle_enabled(
            &job.app_state.app_settings_path,
        ),
    );
    let request = app_services::import_job_coordinator::SingleImportRequest {
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
    };
    match app_services::import_job_coordinator::run_single_import_with_mode(request, mode) {
        Ok(app_services::import_job_coordinator::SingleImportOutcome::Completed(message)) => {
            if let Some(app) = app {
                event_bridge::emit_job_completed(app, &job.job_id.0, &message);
                // Inline rollback already ran timeline/artifacts inside the job;
                // only Detached admits them to the post-import scheduler.
                if matches!(
                    mode,
                    app_services::import_job_coordinator::SingleImportMode::Detached
                ) {
                    crate::post_import_scheduler::enqueue_for_source(
                        &job.app_state,
                        Some(app),
                        &job.case_id.0,
                        &job.case_root,
                        &source_path,
                    );
                }
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
