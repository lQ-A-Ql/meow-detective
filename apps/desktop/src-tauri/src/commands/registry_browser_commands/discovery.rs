use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryHiveListRequestDto, RegistryHivePageDto},
    CommandError,
};

#[tauri::command]
pub async fn list_registry_hives(
    state: State<'_, AppState>,
    request: RegistryHiveListRequestDto,
) -> Result<RegistryHivePageDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::list_hives(registry, ctx, request)
    })
    .await
}
