use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use app_services::ceph_reconstruction::{DerivedSourceError, MaterializedRbdSource};
use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::{dto::CancellationStateDto, CommandError};

use super::super::cancellation::job_cancellation_dto;
use super::types::{
    BackgroundLinuxClusterImportJob, BrowseableClusterImport, ClusterImportSummary,
};
use crate::events::event_bridge;

pub(super) fn cancel_job(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    message: &str,
) {
    let changed = match job_repo.cancel(job_id, message) {
        Ok(changed) => changed,
        Err(error) => {
            tracing::error!("Failed to mark job {} as cancelled: {}", job_id.0, error);
            false
        }
    };
    if changed {
        let Some(app) = app else {
            return;
        };
        event_bridge::emit_job_cancelled(app, &job_id.0, message);
        event_bridge::emit_job_cancellation(
            app,
            &job_cancellation_dto(&job_id.0, CancellationStateDto::Cancelled, true, message),
        );
    }
}

pub(crate) fn complete_browseable_cluster_job(
    outcome: &BrowseableClusterImport,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&outcome.processing.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let completed = JobRepo::new(&connection)
        .complete_if_active(&outcome.parent_job_id, &outcome.completion_detail)
        .map_err(CommandError::from_typed_service_error)?;
    if !completed {
        return Err(CommandError::conflict(
            "Cluster import job became terminal before derived processing was admitted",
        ));
    }
    if let Some(app) = app {
        event_bridge::emit_job_completed(app, &outcome.parent_job_id.0, &outcome.completion_detail);
    }
    Ok(())
}

pub(crate) fn fail_browseable_cluster_job(
    outcome: &BrowseableClusterImport,
    app: Option<&AppHandle>,
    detail: &str,
) {
    match app_services::connection::open_case_db(&outcome.processing.db_path) {
        Ok(connection) => {
            if let Err(error) = JobRepo::new(&connection).fail(&outcome.parent_job_id, detail) {
                tracing::error!(
                    job_id = %outcome.parent_job_id.0,
                    %error,
                    "Failed to persist derived-task admission failure"
                );
            }
        }
        Err(error) => tracing::error!(
            job_id = %outcome.parent_job_id.0,
            %error,
            "Failed to open the case database for derived-task admission failure"
        ),
    }
    if let Some(app) = app {
        event_bridge::emit_job_failed(app, &outcome.parent_job_id.0, detail);
    }
}

pub(crate) fn cancel_browseable_cluster_job(
    outcome: &BrowseableClusterImport,
    app: Option<&AppHandle>,
    detail: &str,
) {
    match app_services::connection::open_case_db(&outcome.processing.db_path) {
        Ok(connection) => cancel_job(
            &JobRepo::new(&connection),
            &outcome.parent_job_id,
            app,
            detail,
        ),
        Err(error) => tracing::error!(
            job_id = %outcome.parent_job_id.0,
            %error,
            "Failed to open the case database for cluster cancellation"
        ),
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
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<Vec<MaterializedRbdSource>>, CommandError> {
    match app_services::ceph_reconstruction::materialize_rbd_sources_for_cluster_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.cluster_id,
        cancel_token,
    ) {
        Ok(sources) => Ok(Some(sources)),
        Err(DerivedSourceError::ProcessingCancelled) => {
            cancel_job(
                job_repo,
                &job.job_id,
                app,
                "Linux cluster RBD materialization cancelled by user",
            );
            Ok(None)
        }
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
            .map(|()| None)
        }
    }
}

pub(crate) fn continue_cluster_rbd_processing(
    job: &super::types::BackgroundDerivedSourceProcessingJob,
    cancel_token: &Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    for data_source_id in &job.source_ids {
        if cancel_token.load(Ordering::Relaxed) {
            tracing::info!(
                cluster_id = %job.cluster_id,
                data_source_id = %data_source_id.0,
                "Stopped derived-source post-Catalog processing after cancellation"
            );
            return Ok(());
        }
        if let Err(error) =
            app_services::ceph_reconstruction::finalize_rbd_source_processing_with_cancel(
                &connection,
                &job.case_root,
                &job.case_id,
                data_source_id,
                cancel_token.clone(),
            )
        {
            tracing::warn!(
                cluster_id = %job.cluster_id,
                data_source_id = %data_source_id.0,
                error = %error,
                "Browseable RBD source has incomplete background processing"
            );
            return Err(CommandError::internal(error.to_string()));
        }
    }
    Ok(())
}
