mod coordinator;
mod execution;
mod queue;
mod types;

pub(super) use persistence_sqlite::repositories::job_repo::JobRepo;
pub(super) use tauri::AppHandle;
pub(super) use transport::CommandError;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::status::fail_linux_cluster_job;
use super::types::{BackgroundLinuxClusterImportJob, ClusterImportSummary};
use app_services::import_scheduler;
use coordinator::cancel_cluster_members;
use execution::run_member_workers;
use queue::create_member_jobs;
use types::MemberCoordinator;

pub(super) fn import_cluster_members(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxClusterImportJob,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
) -> Result<Option<ClusterImportSummary>, CommandError> {
    let configs = job.plan.member_import_configs();
    let total_members = configs.len() as u32;
    if total_members == 0 {
        return Ok(Some(ClusterImportSummary::new()));
    }
    let scheduling = import_scheduler::ImportSchedulingPolicy::for_workload(
        import_scheduler::ImportWorkload::LinuxCluster {
            member_count: configs.len(),
        },
        job.max_import_workers,
        job.max_analysis_workers,
    );
    tracing::info!(
        cluster_id = %job.plan.cluster_id,
        members = total_members,
        source_concurrency = scheduling.source_concurrency,
        import_workers_per_source = scheduling.import_workers,
        analysis_workers_per_source = scheduling.analysis_workers,
        memory_reservation_mb_per_source = scheduling.memory_reservation_mb,
        "Linux cluster import scheduling policy selected"
    );
    let work = match create_member_jobs(job_repo, job, app, configs) {
        Ok(work) => work,
        Err(error) => {
            return fail_linux_cluster_job(
                job_repo,
                &job.job_id,
                app,
                Some((connection, &job.plan.cluster_id, 0, 0)),
                error,
            )
            .map(|_| None);
        }
    };
    let coordinator = MemberCoordinator {
        connection,
        job_repo,
        job,
        app,
        cancel_token,
        total_members,
    };
    let mut summary = run_member_workers(&coordinator, scheduling, work)?;
    summary.member_messages.sort();
    if cancel_token.load(Ordering::Acquire) {
        cancel_cluster_members(connection, job_repo, job, app, &summary);
        return Ok(None);
    }
    Ok(Some(summary))
}
