use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryHiveOpenRequestDto, RegistryHiveSessionDto},
    CommandError,
};

#[tauri::command]
pub async fn open_registry_hive(
    state: State<'_, AppState>,
    request: RegistryHiveOpenRequestDto,
) -> Result<RegistryHiveSessionDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::open_hive(registry, ctx, request)
    })
    .await
}
