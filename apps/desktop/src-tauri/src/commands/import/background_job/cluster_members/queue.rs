use app_services::import_precheck;
use domain::JobId;

use super::super::types::BackgroundLinuxClusterImportJob;
use super::types::MemberWork;
use super::{AppHandle, CommandError, JobRepo};
use crate::events::event_bridge;

pub(super) fn create_member_jobs(
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    configs: Vec<import_precheck::ImportSourceConfig>,
) -> Result<Vec<MemberWork>, CommandError> {
    let total = configs.len();
    let mut work = Vec::with_capacity(total);
    for (index, config) in configs.into_iter().enumerate() {
        let member_job = match job_repo.create(
            &job.case_id.0,
            &format!("Import Linux cluster member {}/{}", index + 1, total),
        ) {
            Ok(job_id) => job_id,
            Err(error) => {
                cancel_created_member_jobs(job_repo, app, &work);
                return Err(CommandError::from_typed_service_error(error));
            }
        };
        let detail = format!("Queued Linux cluster member: {}", config.source_name);
        if let Err(error) = job_repo.update_progress(&member_job, 1, &detail) {
            cancel_member_job(job_repo, app, &member_job, "Failed to queue cluster member");
            cancel_created_member_jobs(job_repo, app, &work);
            return Err(CommandError::from_typed_service_error(error));
        }
        if let Some(app) = app {
            event_bridge::emit_job_created(app, &member_job.0, &detail);
            event_bridge::emit_job_progress(app, &member_job.0, 1, &detail);
        }
        work.push(MemberWork {
            index,
            config,
            job_id: member_job,
        });
    }
    Ok(work)
}

pub(super) fn cancel_created_member_jobs(
    job_repo: &JobRepo<'_>,
    app: Option<&AppHandle>,
    work: &[MemberWork],
) {
    for member in work {
        cancel_member_job(job_repo, app, &member.job_id, "Cluster member queue failed");
    }
}

pub(super) fn cancel_member_job(
    job_repo: &JobRepo<'_>,
    app: Option<&AppHandle>,
    job_id: &JobId,
    message: &str,
) {
    if let Err(error) = job_repo.cancel(job_id, message) {
        tracing::warn!(job_id = %job_id.0, error = %error, "Failed to cancel cluster member job");
    }
    if let Some(app) = app {
        event_bridge::emit_job_cancelled(app, &job_id.0, message);
    }
}
