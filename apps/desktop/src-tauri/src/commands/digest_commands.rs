use tauri::{AppHandle, State};
use transport::{commands::CalculateEvidenceDigestRequest, dto::EvidenceTaskDto, CommandError};

use crate::state::AppState;

#[tauri::command]
pub async fn list_evidence_digests(
    state: State<'_, AppState>,
    request: transport::dto::DigestResultsRequestDto,
) -> Result<Vec<transport::dto::DigestResultDto>, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = crate::commands::command_support::require_active_case(&state)?;
        let conn = app_services::connection::open_case_db(&active.case_root.join("app.db"))
            .map_err(CommandError::from_typed_service_error)?;
        app_services::digest_service::results::list(
            &conn,
            &active.case_root,
            &domain::CaseId(active.case_id),
            request,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
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
