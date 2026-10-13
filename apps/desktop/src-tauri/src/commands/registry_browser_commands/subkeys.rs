use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistrySubkeyPageDto, RegistrySubkeysRequestDto},
    CommandError,
};

#[tauri::command]
pub async fn list_registry_subkeys(
    state: State<'_, AppState>,
    request: RegistrySubkeysRequestDto,
) -> Result<RegistrySubkeyPageDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::list_subkeys(registry, ctx, request)
    })
    .await
}
