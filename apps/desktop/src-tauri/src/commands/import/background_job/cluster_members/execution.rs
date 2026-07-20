use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use app_services::import_scheduler;

use super::super::super::events::TauriImportEventSink;
use super::super::super::pipeline::{execute_import_job, ImportJobOptions};
use super::super::status::fail_linux_cluster_job;
use super::types::{MemberCoordinator, MemberExecutionContext, MemberResult, MemberWork};
use super::{CommandError, JobRepo};
use crate::events::event_bridge;

pub(super) fn run_member_workers(
    coordinator: &MemberCoordinator<'_, '_>,
    scheduling: import_scheduler::ImportSchedulingPolicy,
    work: Vec<MemberWork>,
) -> Result<super::super::types::ClusterImportSummary, CommandError> {
    let worker_count = scheduling.source_worker_count(work.len());
    let pending = Arc::new(Mutex::new(VecDeque::from(work)));
    let (result_tx, result_rx) = sync_channel(worker_count.max(1));
    let mut summary = super::super::types::ClusterImportSummary::new();
    let collection_result = std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let sender = result_tx.clone();
            let pending = Arc::clone(&pending);
            let execution = MemberExecutionContext {
                db_path: coordinator.job.db_path.clone(),
                case_id: coordinator.job.case_id.clone(),
                case_root: coordinator.job.case_root.clone(),
                app: coordinator.app.cloned(),
                cancel_token: Arc::clone(coordinator.cancel_token),
                scheduling,
                analysis_mode: coordinator.job.analysis_mode,
            };
            scope.spawn(move || loop {
                let member = pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .pop_front();
                let Some(member) = member else {
                    break;
                };
                let member_index = member.index;
                let member_job_id = member.job_id.clone();
                let outcome =
                    catch_unwind(AssertUnwindSafe(|| run_member_import(member, &execution)))
                        .unwrap_or_else(|_| {
                            Err(CommandError::internal("Cluster member import panicked"))
                        });
                if sender
                    .send(MemberResult {
                        index: member_index,
                        job_id: member_job_id,
                        outcome,
                    })
                    .is_err()
                {
                    break;
                }
            });
        }
        drop(result_tx);
        coordinator.collect_member_results(result_rx, &mut summary)
    });
    if let Err(error) = collection_result {
        return fail_linux_cluster_job(
            coordinator.job_repo,
            &coordinator.job.job_id,
            coordinator.app,
            Some((
                coordinator.connection,
                &coordinator.job.plan.cluster_id,
                summary.ready_count,
                summary.failed_count,
            )),
            error,
        )
        .map(|_| summary);
    }
    Ok(summary)
}

fn run_member_import(
    member: MemberWork,
    execution: &MemberExecutionContext,
) -> Result<String, CommandError> {
    let MemberWork { config, job_id, .. } = member;
    let admission = import_scheduler::global_import_admission();
    let _permit = admission
        .acquire(
            execution.scheduling.admission_request(),
            execution.cancel_token.as_ref(),
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let connection = app_services::connection::open_existing_case_db(&execution.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let member_job_repo = JobRepo::new(&connection);
    let snapshot = admission.snapshot();
    let started_detail = format!(
        "Started member import: activeSources={} cpuWeight={}/{} memoryMb={}/{}",
        snapshot.active_sources,
        snapshot.cpu_in_use,
        snapshot.cpu_capacity,
        snapshot.memory_in_use_mb,
        snapshot.memory_capacity_mb
    );
    member_job_repo
        .update_progress(&job_id, 5, &started_detail)
        .map_err(CommandError::from_typed_service_error)?;
    tracing::info!(
        job_id = %job_id.0,
        active_sources = snapshot.active_sources,
        cpu_in_use = snapshot.cpu_in_use,
        memory_in_use_mb = snapshot.memory_in_use_mb,
        peak_active_sources = snapshot.peak_active_sources,
        peak_cpu_in_use = snapshot.peak_cpu_in_use,
        peak_memory_in_use_mb = snapshot.peak_memory_in_use_mb,
        "Linux cluster member admitted by import scheduler"
    );
    if let Some(app) = execution.app.as_ref() {
        event_bridge::emit_job_started(app, &job_id.0, "Linux cluster member import started");
        event_bridge::emit_job_progress(app, &job_id.0, 5, &started_detail);
    }
    let event_sink = execution.app.as_ref().map(TauriImportEventSink::new);
    let options = ImportJobOptions {
        event_sink: event_sink
            .as_ref()
            .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
        cancel_token: &execution.cancel_token,
        max_import_workers: Some(execution.scheduling.import_workers),
        max_analysis_workers: Some(execution.scheduling.analysis_workers),
        analysis_mode: execution.analysis_mode,
    };
    execute_import_job(
        &connection,
        &execution.case_id,
        &execution.case_root,
        config,
        &job_id,
        options,
    )
}
