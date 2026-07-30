use std::{path::PathBuf, time::Duration};

use app_services::case_service;
use tauri::{AppHandle, State};
use transport::{commands::CreateCaseRequest, dto::CaseSummaryDto, CommandError};

use super::{
    lifecycle_support::{initialize_and_remember, meta_to_dto, rollback_created_case},
    transition::begin_active_case_transition,
};
use crate::{events::event_bridge, state::AppState};

#[tauri::command]
pub async fn create_case(
    state: State<'_, AppState>,
    app: AppHandle,
    request: CreateCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let root = PathBuf::from(&request.case_root);
    let name = request.name;
    let examiner = request.examiner;
    let active = tauri::async_runtime::spawn_blocking(move || {
        case_service::create_case(&root, &name, examiner.as_deref())
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;
    let active_case_root = active.case_root.clone();
    let dto = meta_to_dto(&active.meta);
    let transition = match begin_active_case_transition(&app_state, active, Duration::from_secs(5))
    {
        Ok(transition) => transition,
        Err(error) => {
            rollback_created_case(active_case_root).await;
            return Err(error);
        }
    };
    if let Err(error) = initialize_and_remember(&app_state, &active_case_root, &dto) {
        transition.rollback(&app_state, Duration::from_secs(5));
        rollback_created_case(active_case_root).await;
        return Err(error);
    }
    transition.commit(&app_state);
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}
