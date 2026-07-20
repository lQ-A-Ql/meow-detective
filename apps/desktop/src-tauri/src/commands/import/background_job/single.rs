use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::{
    super::{
        cancellation::is_import_cancelled_message,
        events::TauriImportEventSink,
        pipeline::{execute_import_job, ImportJobOptions},
    },
    gate::acquire_import_slot,
    status::{cancel_job, fail_job},
    types::BackgroundImportJob,
};
use crate::events::event_bridge;

pub(crate) fn run_background_import_job(
    job: BackgroundImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);

    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Import started");
    }
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(&job_repo, &job.job_id, app, "Import cancelled by user");
        return Ok(());
    }

    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    let scheduling = app_services::import_scheduler::ImportSchedulingPolicy::for_workload(
        app_services::import_scheduler::ImportWorkload::SingleSource,
        job.max_import_workers,
        job.max_analysis_workers,
    );
    tracing::info!(
        job_id = %job.job_id.0,
        import_workers = scheduling.import_workers,
        analysis_workers = scheduling.analysis_workers,
        source_concurrency = scheduling.source_concurrency,
        memory_reservation_mb = scheduling.memory_reservation_mb,
        "Ordinary import scheduling policy selected"
    );
    let admission = app_services::import_scheduler::global_import_admission();
    let _admission = match admission.acquire(scheduling.admission_request(), &cancel_token) {
        Ok(permit) => permit,
        Err(error) => {
            let message = error.to_string();
            cancel_job(&job_repo, &job.job_id, app, &message);
            return Ok(());
        }
    };
    let snapshot = admission.snapshot();
    let event_sink = app.map(TauriImportEventSink::new);
    tracing::info!(
        job_id = %job.job_id.0,
        active_sources = snapshot.active_sources,
        cpu_in_use = snapshot.cpu_in_use,
        memory_in_use_mb = snapshot.memory_in_use_mb,
        peak_active_sources = snapshot.peak_active_sources,
        peak_cpu_in_use = snapshot.peak_cpu_in_use,
        peak_memory_in_use_mb = snapshot.peak_memory_in_use_mb,
        "Single-source import admitted by import scheduler"
    );
    let options = ImportJobOptions {
        event_sink: event_sink
            .as_ref()
            .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
        cancel_token: &cancel_token,
        max_import_workers: Some(scheduling.import_workers),
        max_analysis_workers: Some(scheduling.analysis_workers),
        analysis_mode: job.analysis_mode,
    };
    match execute_import_job(
        &connection,
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
        Err(error) if is_import_cancelled_message(&error.message) => {
            cancel_job(&job_repo, &job.job_id, app, &error.message);
            Ok(())
        }
        Err(error) => fail_job(&job_repo, &job.job_id, app, error),
    }
}
