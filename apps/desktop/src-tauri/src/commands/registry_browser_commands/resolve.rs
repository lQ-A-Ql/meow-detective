use crate::state::AppState;
use app_services::registry_browser_service;
use tauri::State;
use transport::{
    dto::{RegistryKeyMetadataDto, RegistryKeyResolveRequestDto},
    CommandError,
};

#[tauri::command]
pub async fn resolve_registry_key(
    state: State<'_, AppState>,
    request: RegistryKeyResolveRequestDto,
) -> Result<RegistryKeyMetadataDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, ctx| {
        registry_browser_service::resolve_key(registry, ctx, request)
    })
    .await
}
