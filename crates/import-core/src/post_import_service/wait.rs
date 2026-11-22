use super::{enqueue, ExecutableCapability, PostImportError, PostImportRuntime};
use domain::CaseId;
use persistence_sqlite::repositories::{
    capability_execution_repo::{CapabilityExecutionRepo, CapabilityQueueOutcome},
    source_capability_repo::SourceCapabilityRepo,
};
use source_kernel::task_runtime::TaskPriority;
use std::{path::Path, time::Duration};

/// Submit through the shared executor and wait for the persisted terminal result.
pub fn request_and_wait(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    source: &str,
    kind: ExecutableCapability,
) -> Result<(), PostImportError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let execution = CapabilityExecutionRepo::new(&conn);
    let capabilities = SourceCapabilityRepo::new(&conn);
    let row = match execution.queue_retry(&case.0, source, kind.wire())? {
        CapabilityQueueOutcome::Queued(row) => *row,
        CapabilityQueueOutcome::AlreadyActive => capabilities
            .get(&case.0, source, kind.wire())?
            .ok_or(PostImportError::Interrupted)?,
        CapabilityQueueOutcome::Full => return Err(PostImportError::QueueFull),
    };
    if matches!(row.state.as_str(), "ready" | "notApplicable" | "deferred") {
        return Ok(());
    }
    let revision = row.revision;
    let task_id = format!(
        "{}|{}",
        enqueue::family(&case.0, source, kind),
        row.revision
    );
    if row.state == "running" && !runtime.tasks.is_running(&task_id) {
        return Err(PostImportError::AlreadyActive);
    }
    if matches!(row.state.as_str(), "pending" | "queued") {
        if let Err(error) = enqueue::enqueue(runtime, root, row, TaskPriority::Interactive) {
            execution.cancel_request(&case.0, source, kind.wire(), revision)?;
            return Err(error);
        }
    }
    loop {
        if let Some(result) = runtime
            .tasks
            .wait_task(&task_id, Duration::from_millis(250))
        {
            if let Err(error) = result {
                tracing::error!(error, "requested capability execution failed");
                execution.cancel_request(&case.0, source, kind.wire(), revision)?;
                return Err(PostImportError::ExecutionFailed);
            }
            break;
        }
        if !runtime.tasks.is_running(&task_id) {
            break;
        }
    }
    let row = capabilities
        .get(&case.0, source, kind.wire())?
        .ok_or(PostImportError::Interrupted)?;
    if row.state == "cancelled" && revision.checked_add(1) == Some(row.revision) {
        return Err(PostImportError::Cancelled);
    }
    let expected_revision = if row.state == "ready" {
        revision
            .checked_add(1)
            .ok_or(PostImportError::Interrupted)?
    } else {
        revision
    };
    if row.revision != expected_revision {
        return Err(PostImportError::Interrupted);
    }
    match row.state.as_str() {
        "ready" | "notApplicable" | "deferred" => Ok(()),
        "cancelled" => Err(PostImportError::Cancelled),
        "failed" | "partial" => Err(PostImportError::ExecutionFailed),
        _ => Err(PostImportError::Interrupted),
    }
}
