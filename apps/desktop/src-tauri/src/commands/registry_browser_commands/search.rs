use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{commands::RegistrySearchRequestDto, dto::RegistrySearchPageDto, CommandError};

#[tauri::command]
pub async fn search_registry_hive(
    state: State<'_, AppState>,
    request: RegistrySearchRequestDto,
) -> Result<RegistrySearchPageDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::search_hive(registry, ctx, request)
    })
    .await
}
