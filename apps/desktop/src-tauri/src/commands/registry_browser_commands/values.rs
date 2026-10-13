use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryValuePageDto, RegistryValuesRequestDto},
    CommandError,
};

#[tauri::command]
pub async fn list_registry_values(
    state: State<'_, AppState>,
    request: RegistryValuesRequestDto,
) -> Result<RegistryValuePageDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::list_values(registry, ctx, request)
    })
    .await
}
