use super::ApplicationEventSink;
use import_core::post_import_service::CaseProjectionPublisher;
use source_kernel::task_runtime::TaskManager;
use std::{path::Path, sync::Arc};

pub(super) struct CaseProjectionRequests {
    pub tasks: Arc<TaskManager>,
    pub events: Option<Arc<dyn ApplicationEventSink>>,
}

impl CaseProjectionPublisher for CaseProjectionRequests {
    fn request(&self, case_id: &str, case_root: &Path) -> std::io::Result<()> {
        crate::case_projection_service::schedule_case_projection(
            &crate::case_projection_service::CaseProjectionRuntime {
                tasks: Arc::clone(&self.tasks),
                events: self.events.clone(),
            },
            case_id,
            case_root,
        )
        .map(|_| ())
        .map_err(|error| std::io::Error::other(error.to_string()))
    }
}
