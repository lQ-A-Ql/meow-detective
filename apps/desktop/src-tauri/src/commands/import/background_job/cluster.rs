use super::{
    cluster_members::import_cluster_members,
    cluster_output::build_derived_processing_job,
    cluster_status::materialize_cluster_rbd_sources,
    gate::acquire_import_slot,
    status::{cancel_job, fail_job, fail_linux_cluster_job},
    types::{BackgroundLinuxClusterImportJob, BrowseableClusterImport, ClusterImportSummary},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use transport::CommandError;

pub(crate) fn run_background_linux_cluster_import_until_browseable(
    job: BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableClusterImport>, CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Linux cluster import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Linux cluster import started");
    }
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            &job_repo,
            &job.job_id,
            app,
            "Linux cluster import cancelled by user",
        );
        return Ok(None);
    }
    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    initialize_cluster(&connection, &job_repo, &job, app)?;
    let Some(summary) = import_cluster_members(&connection, &job_repo, &job, app, &cancel_token)?
    else {
        return Ok(None);
    };
    complete_cluster_import(&connection, &job_repo, &job, app, summary, cancel_token)
}

fn initialize_cluster(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    if let Err(error) =
        cluster_service::register_linux_cluster_import(connection, &job.case_id, &job.plan)
    {
        return fail_linux_cluster_job(
            job_repo,
            &job.job_id,
            app,
            None,
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) = cluster_service::write_linux_cluster_manifest(&job.case_root, &job.plan) {
        return fail_linux_cluster_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.cluster_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) = cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "importing",
        0,
        0,
        None,
    ) {
        return fail_linux_cluster_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.cluster_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    Ok(())
}

fn complete_cluster_import(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    summary: ClusterImportSummary,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableClusterImport>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    if summary.failed_count > 0 {
        return complete_cluster_import_with_failures(
            connection,
            job_repo,
            job,
            app,
            summary,
            total_members,
        )
        .map(|()| None);
    }
    cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "ready",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)?;
    let Some(derived_sources) = materialize_cluster_rbd_sources(
        connection,
        job_repo,
        job,
        app,
        &summary,
        Arc::clone(&cancel_token),
    )?
    else {
        return Ok(None);
    };
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            job_repo,
            &job.job_id,
            app,
            "Linux cluster import cancelled after RBD materialization",
        );
        return Ok(None);
    }
    let derived_source_count = derived_sources.len();
    let completion_detail = format!(
        "Imported Linux cluster {}: {}/{} image(s) ready",
        job.plan.cluster_name, summary.ready_count, total_members
    );
    tracing::info!(
        cluster_id = %job.plan.cluster_id,
        members = total_members,
        imported = summary.ready_count,
        derived_sources = derived_source_count,
        summaries = ?summary.member_messages,
        "Linux cluster import is browseable and awaiting derived-task admission"
    );
    Ok(Some(BrowseableClusterImport {
        processing: build_derived_processing_job(job, derived_sources),
        parent_job_id: job.job_id.clone(),
        completion_detail,
    }))
}

fn complete_cluster_import_with_failures(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    summary: ClusterImportSummary,
    total_members: u32,
) -> Result<(), CommandError> {
    let message = format!(
        "Linux cluster import finished with failures: {}/{} image(s) ready, {} failed",
        summary.ready_count, total_members, summary.failed_count
    );
    cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "failed",
        summary.ready_count,
        summary.failed_count,
        Some(&message),
    )
    .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_outcome_counts(&job.job_id, 0, 0, summary.failed_count, true)
        .map_err(CommandError::from_typed_service_error)?;
    tracing::warn!(
        cluster_id = %job.plan.cluster_id,
        members = total_members,
        imported = summary.ready_count,
        failed = summary.failed_count,
        summaries = ?summary.member_messages,
        "Linux cluster import completed with member failures"
    );
    fail_job(job_repo, &job.job_id, app, CommandError::internal(message))
}
