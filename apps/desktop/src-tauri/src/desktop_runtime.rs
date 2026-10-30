use crate::{cache_invalidation, media_protocol, state::AppState};
use tauri::Manager;

pub(crate) fn run_app(app: tauri::App) {
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app.state::<AppState>().mcp_host.shutdown();
        }
    });
}

pub(crate) fn builder() -> tauri::Builder<tauri::Wry> {
    media_protocol::register(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(portable_updater::PUBLIC_KEY)
                .build(),
        )
        .setup(|app| {
            if let Err(error) = crate::embedded_maintenance::initialize() {
                tracing::warn!(%error, "Embedded maintenance tool unavailable");
            }
            crate::app_update::startup::acknowledge_boot();
            cache_invalidation::register(app.handle().clone());
            let state = app.state::<AppState>().inner().clone();
            state
                .set_app_handle(app.handle().clone())
                .map_err(std::io::Error::other)?;
            tauri::async_runtime::spawn(async move {
                state.mcp_host.clone().initialize(state).await;
            });
            Ok(())
        })
        .manage(AppState::default())
}
