use super::super::{
    status::fail_job,
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use std::sync::{atomic::AtomicBool, Arc};
use tauri::AppHandle;
use transport::CommandError;

pub(super) fn complete_evidence_set_import_with_failures(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    total_members: u32,
    task_manager: Option<Arc<crate::state::TaskManager>>,
) -> Result<(), CommandError> {
    let mdraid_sources = match cluster_service::register_linux_evidence_set_mdraid(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
    ) {
        Ok(sources) if !sources.is_empty() => {
            tracing::info!(
            import_set_id = %job.plan.import_set_id,
            derived_sources = sources.len(),
            "Registered nested MD RAID source catalogs after member import failure"
            );
            Some(sources)
        }
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(
                import_set_id = %job.plan.import_set_id,
                error = %error,
                "Nested MD RAID source registration failed after member import failure"
            );
            None
        }
    };
    let mdraid_registered = mdraid_sources.is_some();
    schedule_mdraid_catalogs(mdraid_sources, job, app, task_manager);
    if mdraid_registered {
        let detail = format!(
            "Linux evidence-set import completed partially: {}/{} member image(s) ready; nested MD RAID catalog scheduled",
            summary.ready_count, total_members
        );
        cluster_service::update_linux_evidence_set_import_state(
            connection,
            &job.plan.import_set_id,
            "failed",
            summary.ready_count,
            summary.failed_count,
            Some(&detail),
        )
        .map_err(CommandError::from_typed_service_error)?;
        job_repo
            .update_outcome_counts(&job.job_id, summary.failed_count, 0, 0, true)
            .map_err(app_services::connection::command_error_from_db)?;
        job_repo
            .complete_if_active(&job.job_id, &detail)
            .map_err(app_services::connection::command_error_from_db)?;
        if let Some(app) = app {
            event_bridge::emit_job_completed(app, &job.job_id.0, &detail);
        }
        tracing::warn!(
            import_set_id = %job.plan.import_set_id,
            members = total_members,
            imported = summary.ready_count,
            failed = summary.failed_count,
            summaries = ?summary.member_messages,
            "Linux evidence-set import completed partially with a nested MD RAID catalog scheduled"
        );
        return Ok(());
    }
    let message = format!(
        "Linux evidence-set import finished with failures: {}/{} image(s) ready, {} failed",
        summary.ready_count, total_members, summary.failed_count
    );
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "failed",
        summary.ready_count,
        summary.failed_count,
        Some(&message),
    )
    .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_outcome_counts(&job.job_id, 0, 0, summary.failed_count, true)
        .map_err(app_services::connection::command_error_from_db)?;
    tracing::warn!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        failed = summary.failed_count,
        summaries = ?summary.member_messages,
        "Linux evidence-set import completed with member failures"
    );
    fail_job(job_repo, &job.job_id, app, CommandError::internal(message))
}

fn schedule_mdraid_catalogs(
    sources: Option<Vec<domain::DataSource>>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    task_manager: Option<Arc<crate::state::TaskManager>>,
) {
    let Some(sources) = sources else { return };
    for source in sources {
        schedule_mdraid_catalog(source, job, app, task_manager.as_ref());
    }
}

fn schedule_mdraid_catalog(
    source: domain::DataSource,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    task_manager: Option<&Arc<crate::state::TaskManager>>,
) {
    let task_id = format!("{}:mdraid-catalog:{}", job.job_id.0, source.id.0);
    let source_id = source.id.clone();
    let task = mdraid_catalog_task(source_id.clone(), job, app);
    let token = Arc::new(AtomicBool::new(false));
    if let Some(task_manager) = task_manager {
        let scope = crate::state::TaskScope::data_source(
            job.case_id.0.clone(),
            source_id.0.clone(),
            &task_id,
        );
        if let Err(error) = task_manager.spawn_scoped_heavy(task_id, scope, token, task) {
            tracing::warn!(data_source_id = %source_id.0, error = %error, "Nested MD RAID catalog task could not be scheduled");
        }
    } else if let Err(error) = task() {
        tracing::warn!(data_source_id = %source_id.0, error, "Nested MD RAID catalog materialization failed after member import failure");
    }
}

fn mdraid_catalog_task(
    source_id: domain::DataSourceId,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
) -> impl FnOnce() -> Result<(), String> + Send + 'static {
    let case_root = job.case_root.clone();
    let db_path = job.db_path.clone();
    let parent_job_id = job.job_id.0.clone();
    let app_handle = app.cloned();
    move || {
        let connection = app_services::connection::open_existing_case_db(&db_path)
            .map_err(|error| error.to_string())?;
        let token = AtomicBool::new(false);
        let result = cluster_service::materialize_mdraid_source_catalog_with_cancel(
            &connection,
            &case_root,
            &source_id,
            &token,
        )
        .map_err(|error| error.to_string());
        if result.is_ok() {
            if let Some(app) = app_handle {
                crate::events::event_bridge::emit_job_completed(
                    &app,
                    &parent_job_id,
                    "Nested MD RAID file tree is ready",
                );
            }
        }
        result
    }
}
