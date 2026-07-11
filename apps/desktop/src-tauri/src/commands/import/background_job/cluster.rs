use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::{
    cluster_members::import_cluster_members,
    gate::acquire_import_slot,
    status::{cancel_job, fail_linux_cluster_job},
    types::{BackgroundLinuxClusterImportJob, ClusterImportSummary},
};
use crate::events::event_bridge;

pub(crate) fn run_background_linux_cluster_import_job(
    job: BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
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
        return Ok(());
    }

    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    initialize_cluster(&connection, &job_repo, &job, app)?;
    let Some(summary) = import_cluster_members(&connection, &job_repo, &job, app, &cancel_token)?
    else {
        return Ok(());
    };
    complete_cluster_import(&connection, &job_repo, &job, app, summary)
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
) -> Result<(), CommandError> {
    let total_members = job.plan.members.len() as u32;
    cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "ready",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)?;
    let message = format!(
        "Imported Linux cluster {}: {}/{} image(s) ready",
        job.plan.cluster_name, summary.ready_count, total_members
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
        imported = summary.ready_count,
        summaries = ?summary.member_messages,
        "Linux cluster import completed"
    );
    Ok(())
}
