use crate::state::AppState;
use app_services::registry_browser_service::{
    RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry,
};
use transport::CommandError;

pub(super) async fn run<R: Send + 'static>(
    state: AppState,
    operation: impl for<'a> FnOnce(
            &RegistryBrowserRegistry,
            &RegistryBrowserContext<'a>,
        ) -> Result<R, RegistryBrowserError>
        + Send
        + 'static,
) -> Result<R, CommandError> {
    let _lifecycle = state.case_lifecycle.lock().await;
    let worker = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = crate::commands::command_support::require_active_case(&worker)?;
        let conn = crate::commands::command_support::get_case_connection(&worker)?;
        let context = RegistryBrowserContext {
            case_conn: &conn,
            case_root: &active.case_root,
            case_id: &active.meta.id,
            bitlocker: &worker.bitlocker_runtime,
            preview: &worker.preview_runtime,
        };
        operation(&worker.registry_browser, &context)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
