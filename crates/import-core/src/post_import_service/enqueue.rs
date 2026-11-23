use super::{
    events, CapabilityWork, ExecutableCapability, PostImportError, PostImportRuntime, RetryOutcome,
};
use domain::CaseId;
use persistence_sqlite::repositories::{
    capability_execution_repo::{CapabilityExecutionRepo, CapabilityQueueOutcome},
    source_capability_repo::{SourceCapabilityRepo, SourceCapabilityRow},
};
use source_kernel::task_runtime::{TaskPriority, TaskRegistrationError, TaskScope};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

pub fn enqueue_case_pending(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
) -> Result<usize, PostImportError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let rows = SourceCapabilityRepo::new(&conn).list_pending(&case.0)?;
    let mut accepted = 0;
    for row in rows {
        if ExecutableCapability::parse(&row.capability).is_some()
            && enqueue(runtime, root, row, TaskPriority::Background)?
        {
            accepted += 1;
        }
    }
    Ok(accepted)
}

pub fn retry_capability(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    source: &str,
    kind: ExecutableCapability,
) -> Result<RetryOutcome, PostImportError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let repo = CapabilityExecutionRepo::new(&conn);
    match repo.queue_retry(&case.0, source, kind.wire())? {
        CapabilityQueueOutcome::AlreadyActive => Ok(RetryOutcome::AlreadyActive),
        CapabilityQueueOutcome::Full => Ok(RetryOutcome::Full),
        CapabilityQueueOutcome::Queued(row) => {
            let revision = row.revision;
            if let Err(error) = enqueue(runtime, root, *row, TaskPriority::Interactive) {
                repo.cancel_request(&case.0, source, kind.wire(), revision)?;
                return Err(error);
            }
            Ok(RetryOutcome::Accepted)
        }
    }
}

pub fn cancel_capability(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    source: &str,
    kind: ExecutableCapability,
) -> Result<bool, PostImportError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let changed = CapabilityExecutionRepo::new(&conn).cancel(&case.0, source, kind.wire())?;
    runtime.tasks.cancel(&family(&case.0, source, kind));
    if changed {
        if let Some(row) = SourceCapabilityRepo::new(&conn).get(&case.0, source, kind.wire())? {
            events::status(runtime.events.as_deref(), &row)?;
        }
    }
    Ok(changed)
}

pub(super) fn enqueue(
    runtime: &PostImportRuntime,
    root: &Path,
    row: SourceCapabilityRow,
    priority: TaskPriority,
) -> Result<bool, PostImportError> {
    let kind = ExecutableCapability::parse(&row.capability).ok_or_else(|| {
        persistence_sqlite::DbError::System("capability is not executable".into())
    })?;
    let group = family(&row.case_id, &row.data_source_id, kind);
    let task_id = format!("{group}|{}", row.revision);
    let scope = TaskScope::data_source(&row.case_id, &row.data_source_id, &group);
    let cancel = Arc::new(AtomicBool::new(false));
    let task_cancel = Arc::clone(&cancel);
    let work = CapabilityWork {
        row: row.clone(),
        case_root: root.to_path_buf(),
        kind,
        bitlocker: Arc::clone(&runtime.bitlocker),
        events: runtime.events.clone(),
        timeline: Arc::clone(&runtime.timeline),
        case_projections: Arc::clone(&runtime.case_projections),
    };
    events::status(runtime.events.as_deref(), &row)?;
    match runtime
        .tasks
        .spawn_scoped_parallel(task_id, scope, priority, cancel, move || {
            super::execution::run(work, task_cancel).map_err(|error| error.to_string())
        }) {
        Ok(()) => Ok(true),
        Err(TaskRegistrationError::DuplicateTaskId(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn family(case: &str, source: &str, kind: ExecutableCapability) -> String {
    format!(
        "{}{case}|{source}|{}",
        super::CAPABILITY_TASK_GROUP_PREFIX,
        kind.wire()
    )
}
