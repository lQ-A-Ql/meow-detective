use tauri::{ipc::Channel, AppHandle};
use transport::{
    dto::{AppUpdateInfoDto, AppUpdateProgressDto},
    CommandError,
};

#[tauri::command]
pub fn get_app_update_info(app: AppHandle) -> Result<AppUpdateInfoDto, CommandError> {
    crate::app_update::info(&app)
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdateInfoDto, CommandError> {
    crate::app_update::check(app).await
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    version: String,
    progress: Channel<AppUpdateProgressDto>,
) -> Result<(), CommandError> {
    crate::app_update::install(app, version, progress).await
}
