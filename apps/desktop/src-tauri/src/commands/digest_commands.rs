use tauri::State;
use transport::{commands::CalculateEvidenceDigestRequest, dto::EvidenceDigestDto, CommandError};

use crate::state::AppState;

/// Calculates a case-scoped evidence digest on a blocking worker.
#[tauri::command]
pub async fn calculate_evidence_digest(
    state: State<'_, AppState>,
    request: CalculateEvidenceDigestRequest,
) -> Result<EvidenceDigestDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        app_services::digest_service::calculate_evidence_digest(
            &connection,
            &active.case_root,
            &active.meta.id,
            request.scope,
            request.algorithm,
            request.data_source_id.as_deref(),
            request.file_id.as_deref(),
            request.partition_index,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
