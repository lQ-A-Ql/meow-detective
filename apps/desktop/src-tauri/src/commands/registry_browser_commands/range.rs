use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryValueRangeDto, RegistryValueRangeRequestDto},
    CommandError,
};

#[tauri::command]
pub async fn read_registry_value_range(
    state: State<'_, AppState>,
    request: RegistryValueRangeRequestDto,
) -> Result<RegistryValueRangeDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::read_value_range(registry, ctx, request)
    })
    .await
}
