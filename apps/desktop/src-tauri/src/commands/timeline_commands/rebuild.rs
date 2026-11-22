use super::super::command_support::require_active_case;
use crate::{post_import_scheduler, state::AppState};
use tauri::AppHandle;
use transport::{dto::TimelineProjectionStatusDto, CommandError};

pub(super) async fn run(
    state: AppState,
    app: AppHandle,
    source: Option<String>,
) -> Result<TimelineProjectionStatusDto, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&state)?;
        app_services::post_import_service::rebuild_timeline(
            &post_import_scheduler::runtime(&state, Some(&app)),
            &active.meta.id,
            &active.case_root,
            source.as_deref(),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
