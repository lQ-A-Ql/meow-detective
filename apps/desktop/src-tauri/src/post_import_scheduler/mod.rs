//! Desktop event adapter for application-owned post-import processing.

use crate::state::AppState;
use app_services::post_import_service::{self, CapabilityEventSink, PostImportRuntime};
use domain::CaseId;
use std::{path::Path, sync::Arc};
use tauri::AppHandle;
use transport::{events::EventEnvelope, CommandError};

pub(crate) use post_import_service::{
    ExecutableCapability as QueuedCapability, RetryOutcome as Admission,
};

struct DesktopCapabilityEvents(AppHandle);
impl CapabilityEventSink for DesktopCapabilityEvents {
    fn emit(&self, event: &EventEnvelope<serde_json::Value>) -> std::io::Result<()> {
        crate::events::event_bridge::emit_event(&self.0, event.topic.as_str(), event)
            .map_err(std::io::Error::other)
    }
}

pub(crate) fn runtime(state: &AppState, app: Option<&AppHandle>) -> PostImportRuntime {
    PostImportRuntime {
        tasks: Arc::clone(&state.task_manager),
        bitlocker: Arc::clone(&state.bitlocker_runtime),
        events: app.map(|app| {
            Arc::new(DesktopCapabilityEvents(app.clone())) as Arc<dyn CapabilityEventSink>
        }),
    }
}

pub(crate) fn enqueue_case_pending(
    state: &AppState,
    app: Option<&AppHandle>,
    case: &str,
    root: &Path,
) -> Result<usize, CommandError> {
    post_import_service::enqueue_case_pending(&runtime(state, app), &CaseId(case.into()), root)
        .map_err(CommandError::from_typed_service_error)
}

pub(crate) fn retry_admission(
    state: &AppState,
    app: Option<&AppHandle>,
    case: &str,
    root: &Path,
    source: &str,
    kind: QueuedCapability,
) -> Result<Admission, CommandError> {
    post_import_service::retry_capability(
        &runtime(state, app),
        &CaseId(case.into()),
        root,
        source,
        kind,
    )
    .map_err(CommandError::from_typed_service_error)
}

pub(crate) fn cancel_source(
    state: &AppState,
    app: Option<&AppHandle>,
    case: &str,
    root: &Path,
    source: &str,
    kind: QueuedCapability,
) -> Result<bool, CommandError> {
    post_import_service::cancel_capability(
        &runtime(state, app),
        &CaseId(case.into()),
        root,
        source,
        kind,
    )
    .map_err(CommandError::from_typed_service_error)
}
