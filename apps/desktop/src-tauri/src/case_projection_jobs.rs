use app_services::case_projection_service::{schedule_case_projection, CaseProjectionRuntime};
use std::{path::Path, sync::Arc};
use tauri::AppHandle;
use transport::CommandError;

use crate::state::TaskManager;

pub(crate) fn schedule(
    task_manager: &Arc<TaskManager>,
    app: &AppHandle,
    case_id: &str,
    case_root: &Path,
) -> Result<Option<String>, CommandError> {
    schedule_case_projection(
        &CaseProjectionRuntime {
            tasks: Arc::clone(task_manager),
            events: Some(crate::post_import_scheduler::event_sink(app)),
        },
        case_id,
        case_root,
    )
    .map_err(CommandError::from_typed_service_error)
}
