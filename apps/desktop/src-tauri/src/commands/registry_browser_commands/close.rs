use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{dto::RegistryHiveCloseRequestDto, CommandError};

#[tauri::command]
pub async fn close_registry_hive(
    state: State<'_, AppState>,
    request: RegistryHiveCloseRequestDto,
) -> Result<(), CommandError> {
    let _lifecycle = state.case_lifecycle.clone().read_owned().await;
    let case_id = state
        .active_case
        .lock()
        .map_err(|_| CommandError::internal("Case state is unavailable"))?
        .as_ref()
        .map(|case| case.meta.id.0.clone());
    if let Some(case_id) = case_id {
        registry_browser_service::close_hive(&state.registry_browser, &case_id, request)
            .map_err(CommandError::from_typed_service_error)?;
    }
    Ok(())
}
