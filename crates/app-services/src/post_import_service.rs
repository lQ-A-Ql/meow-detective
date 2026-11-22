//! Application wiring for scoped post-import processing.

use domain::CaseId;
use source_kernel::task_runtime::TaskManager;
use std::{path::Path, sync::Arc};

mod rebuild;
mod timeline;

pub use rebuild::rebuild_timeline;

pub use import_core::post_import_service::{
    CapabilityEventSink, ExecutableCapability, PostImportError, RetryOutcome,
};

#[derive(Clone)]
pub struct PostImportRuntime {
    pub tasks: Arc<TaskManager>,
    pub bitlocker: Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    pub events: Option<Arc<dyn CapabilityEventSink>>,
}

impl PostImportRuntime {
    fn core(&self) -> import_core::post_import_service::PostImportRuntime {
        import_core::post_import_service::PostImportRuntime {
            tasks: Arc::clone(&self.tasks),
            bitlocker: Arc::clone(&self.bitlocker),
            events: self.events.clone(),
            timeline: Arc::new(timeline::SourceTimelineProjection),
        }
    }
}

pub fn enqueue_case_pending(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
) -> Result<usize, PostImportError> {
    import_core::post_import_service::enqueue_case_pending(&runtime.core(), case, root)
}

pub fn retry_capability(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    source: &str,
    kind: ExecutableCapability,
) -> Result<RetryOutcome, PostImportError> {
    import_core::post_import_service::retry_capability(&runtime.core(), case, root, source, kind)
}

pub fn cancel_capability(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    source: &str,
    kind: ExecutableCapability,
) -> Result<bool, PostImportError> {
    import_core::post_import_service::cancel_capability(&runtime.core(), case, root, source, kind)
}
