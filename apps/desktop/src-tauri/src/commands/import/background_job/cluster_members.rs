use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::{
    super::{
        cancellation::is_import_cancelled_message,
        events::TauriImportEventSink,
        pipeline::{execute_import_job, ImportJobOptions},
    },
    status::cancel_job,
    types::{BackgroundLinuxClusterImportJob, ClusterImportSummary},
};
use crate::events::event_bridge;

enum MemberFailureAction {
    Continue,
    StopCancelled,
}

pub(super) fn import_cluster_members(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
) -> Result<Option<ClusterImportSummary>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    let event_sink = app.map(TauriImportEventSink::new);
    let mut summary = ClusterImportSummary::new();

    for (index, import_config) in job.plan.member_import_configs().into_iter().enumerate() {
        if cancel_token.load(Ordering::Relaxed) {
            cancel_cluster_members(connection, job_repo, job, app, &summary);
            return Ok(None);
        }
        emit_member_progress(
            job_repo,
            job,
            app,
            index,
            total_members,
            &import_config.source_name,
        )?;
        let options = ImportJobOptions {
            event_sink: event_sink
                .as_ref()
                .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
            cancel_token,
            max_import_workers: job.max_import_workers,
            max_analysis_workers: job.max_analysis_workers,
            analysis_mode: job.analysis_mode,
        };
        match execute_import_job(
            connection,
            &job.case_id,
            &job.case_root,
            import_config,
            &job.job_id,
            options,
        ) {
            Ok(message) => record_member_success(connection, job, &mut summary, message)?,
            Err(error) => {
                if matches!(
                    handle_member_failure(connection, job_repo, job, app, &mut summary, error),
                    MemberFailureAction::StopCancelled
                ) {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(summary))
}

fn emit_member_progress(
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    index: usize,
    total_members: u32,
    source_name: &str,
) -> Result<(), CommandError> {
    let progress = 10 + ((index as u32).saturating_mul(80) / total_members.max(1));
    let detail = format!(
        "Importing Linux cluster member {}/{}: {}",
        index + 1,
        total_members,
        source_name
    );
    job_repo
        .update_progress(&job.job_id, progress.min(90), &detail)
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job.job_id.0, progress.min(90), &detail);
    }
    Ok(())
}

fn record_member_success(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxClusterImportJob,
    summary: &mut ClusterImportSummary,
    message: String,
) -> Result<(), CommandError> {
    summary.ready_count = summary.ready_count.saturating_add(1);
    summary.member_messages.push(message);
    cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "importing",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)
}

fn handle_member_failure(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    summary: &mut ClusterImportSummary,
    error: CommandError,
) -> MemberFailureAction {
    summary.failed_count = summary.failed_count.saturating_add(1);
    summary.member_messages.push(error.message.clone());
    let _ = cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "importing",
        summary.ready_count,
        summary.failed_count,
        Some(&error.message),
    );
    if is_import_cancelled_message(&error.message) {
        cancel_job(job_repo, &job.job_id, app, &error.message);
        MemberFailureAction::StopCancelled
    } else {
        tracing::warn!(
            cluster_id = %job.plan.cluster_id,
            ready_count = summary.ready_count,
            failed_count = summary.failed_count,
            error = %error.message,
            "Linux cluster member import failed; continuing with remaining members"
        );
        MemberFailureAction::Continue
    }
}

fn cancel_cluster_members(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    summary: &ClusterImportSummary,
) {
    let message = "Linux cluster import cancelled by user";
    let _ = cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "cancelled",
        summary.ready_count,
        summary.failed_count,
        Some(message),
    );
    cancel_job(job_repo, &job.job_id, app, message);
}
