use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::{dto::CancellationStateDto, CommandError};

use super::super::cancellation::job_cancellation_dto;
use super::types::{BackgroundLinuxClusterImportJob, ClusterImportSummary};
use crate::events::event_bridge;

pub(super) fn cancel_job(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    message: &str,
) {
    if let Err(error) = job_repo.cancel(job_id, message) {
        tracing::error!("Failed to mark job {} as cancelled: {}", job_id.0, error);
    }
    if let Some(app) = app {
        event_bridge::emit_job_cancelled(app, &job_id.0, message);
        event_bridge::emit_job_cancellation(
            app,
            &job_cancellation_dto(&job_id.0, CancellationStateDto::Cancelled, true, message),
        );
    }
}

pub(super) fn fail_job(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    error: CommandError,
) -> Result<(), CommandError> {
    if let Err(update_error) = job_repo.fail(job_id, &error.message) {
        tracing::error!(
            "Failed to mark job {} as failed: {}",
            job_id.0,
            update_error
        );
    }
    if let Some(app) = app {
        event_bridge::emit_job_failed(app, &job_id.0, &error.message);
    }
    Err(error)
}

pub(super) fn fail_linux_cluster_job(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    cluster_state: Option<(&rusqlite::Connection, &str, u32, u32)>,
    error: CommandError,
) -> Result<(), CommandError> {
    let detail = error.message.clone();
    if let Some((connection, cluster_id, ready_count, failed_count)) = cluster_state {
        if let Err(update_error) = cluster_service::update_linux_cluster_import_state(
            connection,
            cluster_id,
            "failed",
            ready_count,
            failed_count,
            Some(&detail),
        ) {
            tracing::error!(
                cluster_id,
                error = %update_error,
                "Failed to mark Linux cluster import as failed"
            );
        }
    }
    if let Err(update_error) = job_repo.fail(job_id, &detail) {
        tracing::error!(
            job_id = %job_id.0,
            error = %update_error,
            "Failed to mark Linux cluster job as failed"
        );
    }
    if let Some(app) = app {
        event_bridge::emit_job_failed(app, &job_id.0, &detail);
    }
    Err(error)
}

pub(super) fn materialize_cluster_rbd_sources(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    summary: &ClusterImportSummary,
) -> Result<usize, CommandError> {
    match app_services::ceph_reconstruction::materialize_rbd_sources_for_cluster(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.cluster_id,
    ) {
        Ok(sources) => Ok(sources.len()),
        Err(error) => {
            let message = format!(
                "Linux cluster {} RBD materialization failed: {error}",
                job.plan.cluster_name
            );
            fail_linux_cluster_job(
                job_repo,
                &job.job_id,
                app,
                Some((
                    connection,
                    &job.plan.cluster_id,
                    summary.ready_count,
                    summary.failed_count,
                )),
                CommandError::internal(message),
            )
            .map(|()| 0)
        }
    }
}
