use super::scheduler::{emit, CaseProjectionRuntime, CaseProjectionScheduleError, Requests};
use super::{CaseProjectionBuildReport, CaseProjectionError, CaseProjectionJobOutcome};
use import_core::post_import_service::CAPABILITY_TASK_GROUP_PREFIX;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use transport::events::EventTopic;

pub(super) fn run(
    runtime: &CaseProjectionRuntime,
    case_id: &str,
    root: &Path,
    job: &domain::JobId,
    requests: &Mutex<Requests>,
    cancel: &AtomicBool,
) -> Result<(), CaseProjectionScheduleError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let repo = JobRepo::new(&conn);
    if !repo.start(job)? {
        return Ok(());
    }
    let result = (|| {
        emit(
            runtime,
            case_id,
            &job.0,
            EventTopic::JobStarted,
            serde_json::json!({"detail":"Building case projections"}),
        )?;
        build_latest(runtime, &conn, case_id, root, job, requests, cancel)
    })();
    match super::finish_case_projection_job(&conn, job, result)? {
        CaseProjectionJobOutcome::Completed(detail) => {
            emit(
                runtime,
                case_id,
                &job.0,
                EventTopic::JobCompleted,
                serde_json::json!({"message": detail}),
            )?;
            Ok(())
        }
        CaseProjectionJobOutcome::Cancelled => {
            emit(
                runtime,
                case_id,
                &job.0,
                EventTopic::JobCancelled,
                serde_json::json!({"reason": "Case projection build cancelled"}),
            )?;
            Err(
                std::io::Error::new(std::io::ErrorKind::Interrupted, "case projection cancelled")
                    .into(),
            )
        }
        CaseProjectionJobOutcome::Failed(detail) => {
            emit(
                runtime,
                case_id,
                &job.0,
                EventTopic::JobFailed,
                serde_json::json!({"error": detail}),
            )?;
            Err(std::io::Error::other("case projection build failed").into())
        }
    }
}

fn build_latest(
    runtime: &CaseProjectionRuntime,
    conn: &rusqlite::Connection,
    case_id: &str,
    root: &Path,
    job: &domain::JobId,
    requests: &Mutex<Requests>,
    cancel: &AtomicBool,
) -> Result<CaseProjectionBuildReport, CaseProjectionError> {
    loop {
        if let Err(error) =
            runtime
                .tasks
                .wait_for_group_prefix(case_id, CAPABILITY_TASK_GROUP_PREFIX, cancel)
        {
            if cancel.load(Ordering::Acquire) {
                return Err(CaseProjectionError::Cancelled);
            }
            return Err(error.into());
        }
        let epoch = requests
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .epoch;
        let _budget = source_kernel::resource_budget::global_pool()
            .acquire(
                source_kernel::resource_budget::Resources {
                    bytes: 384 * source_kernel::resource_budget::MIB,
                    workers: 1,
                    ..Default::default()
                },
                Some(cancel),
            )
            .map_err(|error| {
                if matches!(
                    error,
                    source_kernel::resource_budget::ResourceError::Cancelled
                ) {
                    CaseProjectionError::Cancelled
                } else {
                    CaseProjectionError::Runtime(error.into_io())
                }
            })?;
        let report = super::rebuild_case_projections(conn, root, case_id, cancel)?;
        let mut state = requests
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        if state.epoch == epoch {
            if state.active_job.as_deref() == Some(&job.0) {
                state.active_job = None;
            }
            return Ok(report);
        }
    }
}
