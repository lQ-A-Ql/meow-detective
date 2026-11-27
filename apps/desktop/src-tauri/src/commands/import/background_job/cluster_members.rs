mod coordinator;
mod deferred;
mod execution;
mod queue;
pub(super) mod types;

pub(super) use persistence_sqlite::repositories::job_repo::JobRepo;
pub(super) use tauri::AppHandle;
pub(super) use transport::CommandError;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::status::fail_linux_evidence_set_job;
use super::types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary};
use crate::state::TaskManager;
use app_services::import_scheduler;
use coordinator::cancel_evidence_set_members;
use execution::run_member_workers;
use queue::create_member_jobs;
use types::MemberCoordinator;

pub(super) fn import_evidence_set_members(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
    collection_plan: Arc<app_services::datasource_service::ImageCollectionPlan>,
    task_manager: Option<Arc<TaskManager>>,
) -> Result<Option<EvidenceSetImportSummary>, CommandError> {
    let configs = match job.plan.member_import_configs_from_plan(collection_plan) {
        Ok(configs) => configs,
        Err(error) => {
            let error = CommandError::from_typed_service_error(error);
            if error.category == "cancelled" {
                cancel_evidence_set_members(
                    connection,
                    job_repo,
                    job,
                    app,
                    &EvidenceSetImportSummary::new(),
                );
                return Ok(None);
            }
            return fail_linux_evidence_set_job(
                job_repo,
                &job.job_id,
                app,
                Some((connection, &job.plan.import_set_id, 0, 0)),
                error,
            )
            .map(|()| None);
        }
    };
    let total_members = configs.len() as u32;
    let collection_plan = configs
        .first()
        .and_then(|config| config.collection_plan.clone());
    if total_members == 0 {
        return Ok(Some(EvidenceSetImportSummary::new()));
    }
    let scheduling = import_scheduler::ImportSchedulingPolicy::for_workload(
        import_scheduler::ImportWorkload::LinuxEvidenceSet {
            member_count: configs.len(),
        },
        job.max_import_workers,
        job.max_analysis_workers,
    );
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        source_concurrency = scheduling.source_concurrency,
        import_workers_per_source = scheduling.import_workers,
        analysis_workers_per_source = scheduling.analysis_workers,
        memory_reservation_mb_per_source = scheduling.memory_reservation_mb,
        "Linux evidence-set import scheduling policy selected"
    );
    let work = match create_member_jobs(job_repo, job, app, configs) {
        Ok(work) => work,
        Err(error) => {
            return fail_linux_evidence_set_job(
                job_repo,
                &job.job_id,
                app,
                Some((connection, &job.plan.import_set_id, 0, 0)),
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
        task_manager,
    };
    let mut summary = run_member_workers(&coordinator, scheduling, work)?;
    if !verify_collection(&coordinator, collection_plan.as_deref(), &summary)? {
        return Ok(None);
    }
    summary.collection_plan = collection_plan;
    summary.member_messages.sort();
    if cancel_token.load(Ordering::Acquire) {
        cancel_evidence_set_members(connection, job_repo, job, app, &summary);
        return Ok(None);
    }
    Ok(Some(summary))
}

pub(super) fn verify_collection(
    coordinator: &MemberCoordinator<'_, '_>,
    plan: Option<&app_services::datasource_service::ImageCollectionPlan>,
    summary: &EvidenceSetImportSummary,
) -> Result<bool, CommandError> {
    let Some(plan) = plan else {
        return Err(CommandError::internal("Collection image plan missing"));
    };
    match app_services::cluster_service::validate_collection_publication(
        coordinator.connection,
        &coordinator.job.case_root,
        &coordinator.job.case_id,
        &coordinator.job.plan,
        plan,
        coordinator.cancel_token,
    ) {
        Ok(()) => Ok(true),
        Err(_) if coordinator.cancel_token.load(Ordering::Acquire) => {
            cancel_evidence_set_members(
                coordinator.connection,
                coordinator.job_repo,
                coordinator.job,
                coordinator.app,
                summary,
            );
            Ok(false)
        }
        Err(error) => fail_linux_evidence_set_job(
            coordinator.job_repo,
            &coordinator.job.job_id,
            coordinator.app,
            Some((
                coordinator.connection,
                &coordinator.job.plan.import_set_id,
                summary.ready_count,
                summary.failed_count,
            )),
            CommandError::from_typed_service_error(error),
        )
        .map(|()| false),
    }
}
