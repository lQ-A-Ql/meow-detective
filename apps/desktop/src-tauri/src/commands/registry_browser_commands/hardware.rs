use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryHardwareRequestDto, RegistryHardwareSummaryDto},
    CommandError,
};

#[tauri::command]
pub async fn get_registry_hardware(
    state: State<'_, AppState>,
    request: RegistryHardwareRequestDto,
) -> Result<RegistryHardwareSummaryDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::read_hardware(registry, ctx, request)
    })
    .await
}
