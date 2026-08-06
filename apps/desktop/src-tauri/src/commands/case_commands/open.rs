use std::{path::PathBuf, time::Duration};

use app_services::case_service;
use tauri::{AppHandle, State};
use transport::{commands::OpenCaseRequest, dto::CaseSummaryDto, CommandError};

use super::{
    lifecycle_support::{initialize_and_remember, meta_to_dto},
    open_restore::restore_enabled_bitlocker_volumes,
    recovery::recover_interrupted_jobs,
    transition::begin_active_case_transition,
};
use crate::{events::event_bridge, state::AppState};

#[tauri::command]
pub async fn open_case(
    state: State<'_, AppState>,
    app: AppHandle,
    request: OpenCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let root = PathBuf::from(&request.case_root);
    let open_root = root.clone();
    let active = tauri::async_runtime::spawn_blocking(move || {
        case_service::open_case(&open_root).map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    let dto = meta_to_dto(&active.meta);
    let transition = begin_active_case_transition(&app_state, active, Duration::from_secs(5))?;
    if let Err(error) = initialize_and_remember(&app_state, &root, &dto) {
        transition.rollback(&app_state, Duration::from_secs(5));
        return Err(error);
    }
    restore_enabled_bitlocker_volumes(&app_state, &root, &dto).await;
    transition.commit(&app_state);
    recover_interrupted_jobs(&app_state);
    if let Err(error) = crate::commands::import::background_job::schedule_pending_evidence_hashes(
        &root,
        &dto.id,
        Some(&app),
        app_state.task_manager.clone(),
    ) {
        tracing::warn!(error = %error.message, "Failed to schedule pending evidence hash jobs after case open");
    }
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}
