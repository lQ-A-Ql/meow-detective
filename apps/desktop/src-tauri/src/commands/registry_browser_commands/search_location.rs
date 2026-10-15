use crate::state::AppState;
use tauri::State;
use transport::{
    dto::{RegistrySearchLocateRequestDto, RegistrySearchLocationDto},
    CommandError,
};
#[tauri::command]
pub async fn locate_registry_search_hit(
    state: State<'_, AppState>,
    request: RegistrySearchLocateRequestDto,
) -> Result<RegistrySearchLocationDto, CommandError> {
    super::support::run(state.inner().clone(), move |registry, context| {
        app_services::registry_browser_service::locate_search_hit(registry, context, request)
    })
    .await
}
