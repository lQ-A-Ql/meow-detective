use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use app_services::derived_source_service::{DerivedSourceError, MaterializedRbdSource};
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::status::{cancel_job, fail_linux_evidence_set_job};
use super::types::{
    BackgroundDerivedSourceProcessingJob, BackgroundLinuxEvidenceSetImportJob,
    BrowseableEvidenceSetImport, EvidenceSetImportSummary,
};
use crate::events::event_bridge;

pub(crate) fn complete_browseable_evidence_set_job(
    outcome: &BrowseableEvidenceSetImport,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&outcome.processing.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let completed = JobRepo::new(&connection)
        .complete_if_active(&outcome.parent_job_id, &outcome.completion_detail)
        .map_err(CommandError::from_typed_service_error)?;
    if !completed {
        return Err(CommandError::conflict(
            "Evidence-set import job became terminal before derived processing was admitted",
        ));
    }
    if let Some(app) = app {
        event_bridge::emit_job_completed(app, &outcome.parent_job_id.0, &outcome.completion_detail);
    }
    Ok(())
}

pub(crate) fn fail_browseable_evidence_set_job(
    outcome: &BrowseableEvidenceSetImport,
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

pub(crate) fn cancel_browseable_evidence_set_job(
    outcome: &BrowseableEvidenceSetImport,
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

pub(super) fn materialize_ceph_scope_rbd_sources(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    ceph_scope_id: &domain::CephScopeId,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<Vec<MaterializedRbdSource>>, CommandError> {
    match app_services::derived_source_service::materialize_rbd_sources_for_scope_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        ceph_scope_id,
        cancel_token,
    ) {
        Ok(sources) => Ok(Some(sources)),
        Err(DerivedSourceError::ProcessingCancelled) => {
            if let Err(error) =
                app_services::cluster_service::update_linux_evidence_set_import_state(
                    connection,
                    &job.plan.import_set_id,
                    "cancelled",
                    summary.ready_count,
                    summary.failed_count,
                    Some("Linux evidence set RBD materialization cancelled by user"),
                )
            {
                tracing::error!(
                    import_set_id = %job.plan.import_set_id,
                    %error,
                    "Failed to mark Linux evidence set cancelled after RBD cancellation"
                );
            }
            cancel_job(
                job_repo,
                &job.job_id,
                app,
                "Linux evidence set RBD materialization cancelled by user",
            );
            Ok(None)
        }
        Err(error) => {
            let message = format!(
                "Linux evidence set {} RBD materialization failed: {error}",
                job.plan.import_set_name
            );
            fail_linux_evidence_set_job(
                job_repo,
                &job.job_id,
                app,
                Some((
                    connection,
                    &job.plan.import_set_id,
                    summary.ready_count,
                    summary.failed_count,
                )),
                CommandError::internal(message),
            )
            .map(|()| None)
        }
    }
}

pub(crate) fn continue_ceph_rbd_processing(
    job: &BackgroundDerivedSourceProcessingJob,
    cancel_token: &Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    for data_source_id in &job.source_ids {
        if cancel_token.load(Ordering::Relaxed) {
            tracing::info!(
                import_set_id = %job.import_set_id,
                data_source_id = %data_source_id.0,
                "Stopped derived-source post-Catalog processing after cancellation"
            );
            return Ok(());
        }
        if let Err(error) =
            app_services::derived_source_service::finalize_rbd_source_processing_with_cancel(
                &connection,
                &job.case_root,
                &job.case_id,
                data_source_id,
                cancel_token.clone(),
            )
        {
            tracing::warn!(
                import_set_id = %job.import_set_id,
                data_source_id = %data_source_id.0,
                error = %error,
                "Browseable RBD source has incomplete background processing"
            );
            return Err(CommandError::internal(error.to_string()));
        }
    }
    Ok(())
}
