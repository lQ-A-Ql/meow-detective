use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use app_services::import_scheduler;

use super::super::super::events::TauriImportEventSink;
use super::super::status::fail_linux_evidence_set_job;
use super::types::{MemberCoordinator, MemberExecutionContext, MemberResult, MemberWork};
use super::CommandError;
use crate::events::event_bridge;

pub(super) fn run_member_workers(
    coordinator: &MemberCoordinator<'_, '_>,
    scheduling: import_scheduler::ImportSchedulingPolicy,
    work: Vec<MemberWork>,
) -> Result<super::super::types::EvidenceSetImportSummary, CommandError> {
    let worker_count = scheduling.source_worker_count(work.len());
    let pending = Arc::new(Mutex::new(VecDeque::from(work)));
    let (result_tx, result_rx) = sync_channel(worker_count.max(1));
    let mut summary = super::super::types::EvidenceSetImportSummary::new();
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
                defer_linux_artifacts: coordinator.task_manager.is_some(),
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
        return fail_linux_evidence_set_job(
            coordinator.job_repo,
            &coordinator.job.job_id,
            coordinator.app,
            Some((
                coordinator.connection,
                &coordinator.job.plan.import_set_id,
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
    let connection = app_services::connection::open_existing_case_db(&execution.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let event_sink = execution.app.as_ref().map(TauriImportEventSink::new);
    match app_services::import_job_coordinator::run_member_import(
        app_services::import_job_coordinator::MemberImportRequest {
            connection: &connection,
            case_id: &execution.case_id,
            case_root: &execution.case_root,
            job_id: &job_id,
            import_config: config,
            scheduling: execution.scheduling,
            analysis_mode: execution.analysis_mode,
            defer_linux_artifacts: execution.defer_linux_artifacts,
            cancel_token: &execution.cancel_token,
            event_sink: event_sink
                .as_ref()
                .map(|sink| sink as &dyn app_services::import_pipeline::ImportEventSink),
        },
    ) {
        Ok(app_services::import_job_coordinator::MemberImportOutcome::Completed(message)) => {
            if let Some(app) = execution.app.as_ref() {
                event_bridge::emit_job_completed(app, &job_id.0, &message);
            }
            Ok(message)
        }
        Ok(app_services::import_job_coordinator::MemberImportOutcome::Cancelled(message)) => {
            Err(CommandError::cancelled(message))
        }
        Err(error) => Err(error),
    }
}
