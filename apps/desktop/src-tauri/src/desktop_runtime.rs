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
        .setup(|app| {
            cache_invalidation::register(app.handle().clone());
            let state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                state.mcp_host.clone().initialize(state).await;
            });
            Ok(())
        })
        .manage(AppState::default())
}
