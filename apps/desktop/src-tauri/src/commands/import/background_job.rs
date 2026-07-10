//! Background import job lifecycle.

use app_services::{cluster_service, import_analysis};
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::Duration;
use tauri::AppHandle;
use transport::{dto::CancellationStateDto, CommandError};

use crate::events::event_bridge;

use super::{
    cancellation::{is_import_cancelled_message, job_cancellation_dto},
    events::TauriImportEventSink,
    pipeline::{execute_import_job, ImportJobOptions},
};

static IMPORT_JOB_GATE: OnceLock<Mutex<()>> = OnceLock::new();

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

pub(crate) struct BackgroundLinuxClusterImportJob {
    pub(crate) db_path: PathBuf,
    pub(crate) case_id: domain::CaseId,
    pub(crate) case_root: PathBuf,
    pub(crate) plan: cluster_service::LinuxClusterImportPlan,
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

    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;

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

/// Run a Linux cluster import as one user-facing job with serial member imports.
pub(crate) fn run_background_linux_cluster_import_job(
    job: BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let conn = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&conn);
    let total_members = job.plan.members.len() as u32;

    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Linux cluster import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Linux cluster import started");
    }

    if cancel_token.load(Ordering::Relaxed) {
        let msg = "Linux cluster import cancelled by user";
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

    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    cluster_service::register_linux_cluster_import(&conn, &job.case_id, &job.plan)
        .map_err(CommandError::from_typed_service_error)?;
    cluster_service::write_linux_cluster_manifest(&job.case_root, &job.plan)
        .map_err(CommandError::from_typed_service_error)?;
    cluster_service::update_linux_cluster_import_state(
        &conn,
        &job.plan.cluster_id,
        "importing",
        0,
        0,
        None,
    )
    .map_err(CommandError::from_typed_service_error)?;

    let event_sink = app.map(TauriImportEventSink::new);
    let mut ready_count = 0u32;
    let mut failed_count = 0u32;
    let mut member_messages = Vec::new();

    for (idx, import_config) in job.plan.member_import_configs().into_iter().enumerate() {
        if cancel_token.load(Ordering::Relaxed) {
            let msg = "Linux cluster import cancelled by user";
            let _ = cluster_service::update_linux_cluster_import_state(
                &conn,
                &job.plan.cluster_id,
                "cancelled",
                ready_count,
                failed_count,
                Some(msg),
            );
            if let Err(e) = job_repo.cancel(&job.job_id, msg) {
                tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
            }
            if let Some(app) = app {
                event_bridge::emit_job_cancelled(app, &job.job_id.0, msg);
                event_bridge::emit_job_cancellation(
                    app,
                    &job_cancellation_dto(
                        &job.job_id.0,
                        CancellationStateDto::Cancelled,
                        true,
                        msg,
                    ),
                );
            }
            return Ok(());
        }

        let progress = 10 + ((idx as u32).saturating_mul(80) / total_members.max(1));
        let detail = format!(
            "Importing Linux cluster member {}/{}: {}",
            idx + 1,
            total_members,
            import_config.source_name
        );
        job_repo
            .update_progress(&job.job_id, progress.min(90), &detail)
            .map_err(CommandError::from_typed_service_error)?;
        if let Some(app) = app {
            event_bridge::emit_job_progress(app, &job.job_id.0, progress.min(90), &detail);
        }

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
            import_config,
            &job.job_id,
            options,
        ) {
            Ok(message) => {
                ready_count = ready_count.saturating_add(1);
                member_messages.push(message);
                cluster_service::update_linux_cluster_import_state(
                    &conn,
                    &job.plan.cluster_id,
                    "importing",
                    ready_count,
                    failed_count,
                    None,
                )
                .map_err(CommandError::from_typed_service_error)?;
            }
            Err(error) => {
                failed_count = failed_count.saturating_add(1);
                let _ = cluster_service::update_linux_cluster_import_state(
                    &conn,
                    &job.plan.cluster_id,
                    "failed",
                    ready_count,
                    failed_count,
                    Some(&error.message),
                );
                if is_import_cancelled_message(&error.message) {
                    if let Err(e) = job_repo.cancel(&job.job_id, &error.message) {
                        tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
                    }
                    if let Some(app) = app {
                        event_bridge::emit_job_cancelled(app, &job.job_id.0, &error.message);
                    }
                    return Ok(());
                }
                if let Err(e) = job_repo.fail(&job.job_id, &error.message) {
                    tracing::error!("Failed to mark job {} as failed: {}", job.job_id.0, e);
                }
                if let Some(app) = app {
                    event_bridge::emit_job_failed(app, &job.job_id.0, &error.message);
                }
                return Err(error);
            }
        }
    }

    cluster_service::update_linux_cluster_import_state(
        &conn,
        &job.plan.cluster_id,
        "ready",
        ready_count,
        failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)?;

    let message = format!(
        "Imported Linux cluster {}: {}/{} image(s) ready",
        job.plan.cluster_name, ready_count, total_members
    );
    job_repo
        .complete(&job.job_id, &message)
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_completed(app, &job.job_id.0, &message);
    }
    tracing::info!(
        cluster_id = %job.plan.cluster_id,
        members = total_members,
        imported = ready_count,
        summaries = ?member_messages,
        "Linux cluster import completed"
    );
    Ok(())
}

fn acquire_import_slot(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
) -> Result<MutexGuard<'static, ()>, CommandError> {
    let gate = IMPORT_JOB_GATE.get_or_init(|| Mutex::new(()));
    let mut emitted_waiting = false;
    loop {
        if cancel_token.load(Ordering::Relaxed) {
            let msg = "Import cancelled while waiting for import slot";
            if let Err(e) = job_repo.cancel(job_id, msg) {
                tracing::error!("Failed to mark job {} as cancelled: {}", job_id.0, e);
            }
            if let Some(app) = app {
                event_bridge::emit_job_cancelled(app, &job_id.0, msg);
                event_bridge::emit_job_cancellation(
                    app,
                    &job_cancellation_dto(&job_id.0, CancellationStateDto::Cancelled, true, msg),
                );
            }
            return Err(CommandError::internal(msg));
        }

        match gate.try_lock() {
            Ok(guard) => {
                if emitted_waiting {
                    job_repo
                        .update_progress(job_id, 5, "Import slot acquired")
                        .map_err(CommandError::from_typed_service_error)?;
                    if let Some(app) = app {
                        event_bridge::emit_job_progress(app, &job_id.0, 5, "Import slot acquired");
                    }
                }
                return Ok(guard);
            }
            Err(TryLockError::WouldBlock) => {
                if !emitted_waiting {
                    job_repo
                        .update_progress(job_id, 2, "Waiting for import slot")
                        .map_err(CommandError::from_typed_service_error)?;
                    if let Some(app) = app {
                        event_bridge::emit_job_progress(
                            app,
                            &job_id.0,
                            2,
                            "Waiting for import slot",
                        );
                    }
                    emitted_waiting = true;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                tracing::warn!("Import job gate was poisoned; continuing with recovered guard");
                return Ok(poisoned.into_inner());
            }
        }
    }
}
