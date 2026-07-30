use std::time::Duration;

use app_services::case_service;
use tauri::{AppHandle, State};
use transport::{dto::CaseSummaryDto, CommandError};

use super::{
    lifecycle_support::{initialize_and_remember, meta_to_dto, rollback_created_case},
    transition::begin_active_case_transition,
};
use crate::{events::event_bridge, state::AppState};

#[tauri::command]
pub async fn create_analysis_demo_case(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<CaseSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let case_root = std::env::temp_dir().join("Meow_Detective-analysis-demo");
    let create_root = case_root.clone();
    let active = tauri::async_runtime::spawn_blocking(move || {
        if create_root.exists() {
            std::fs::remove_dir_all(&create_root).map_err(|error| {
                CommandError::internal(format!("Failed to reset analysis demo case: {error}"))
            })?;
        }
        std::fs::create_dir_all(&create_root).map_err(|error| {
            CommandError::internal(format!("Failed to create analysis demo root: {error}"))
        })?;
        let active = case_service::create_case(&create_root, "Analysis Demo", Some("Codex Demo"))
            .map_err(CommandError::from_typed_service_error)?;
        app_services::analysis_service::seed_analysis_demo_data(&active)
            .map_err(CommandError::from_typed_service_error)?;
        Ok::<_, CommandError>(active)
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
