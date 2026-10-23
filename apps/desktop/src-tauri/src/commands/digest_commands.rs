use tauri::{AppHandle, State};
use transport::{commands::CalculateEvidenceDigestRequest, dto::EvidenceTaskDto, CommandError};

use super::command_support::run_case_read_command;
use crate::state::AppState;

#[tauri::command]
pub async fn list_evidence_digests(
    state: State<'_, AppState>,
    request: transport::dto::DigestResultsRequestDto,
) -> Result<Vec<transport::dto::DigestResultDto>, CommandError> {
    let app_state = state.inner().clone();
    run_case_read_command(app_state, move |conn, active| {
        app_services::digest_service::results::list(
            conn,
            &active.case_root,
            &active.meta.id,
            request,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Every command calculation enters the shared persistent task lifecycle.
#[tauri::command]
pub async fn calculate_evidence_digest(
    state: State<'_, AppState>,
    app: AppHandle,
    request: CalculateEvidenceDigestRequest,
) -> Result<EvidenceTaskDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::evidence_task_jobs::start(
            state,
            app,
            transport::dto::EvidenceTaskRequestDto::Digest(request),
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}
