//! Application settings commands.

use std::path::PathBuf;

use tauri::State;
use transport::{commands::AppSettingsDto, CommandError};

use crate::state::AppState;

/// Get persisted application settings.
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, CommandError> {
    let path = state.inner().app_settings_path.clone();
    tauri::async_runtime::spawn_blocking(move || load_app_settings(&path))
        .await
        .map_err(CommandError::from_join_error)?
}

/// Validate and persist application settings.
#[tauri::command]
pub async fn save_app_settings(
    state: State<'_, AppState>,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    settings.validate().map_err(CommandError::invalid_input)?;
    let path = state.inner().app_settings_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(CommandError::from_typed_service_error)?;
        }
        let content = serde_json::to_string_pretty(&settings)
            .map_err(CommandError::from_typed_service_error)?;
        std::fs::write(&path, content).map_err(CommandError::from_typed_service_error)?;
        Ok(settings)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(crate) fn load_app_settings(path: &PathBuf) -> Result<AppSettingsDto, CommandError> {
    if !path.exists() {
        return Ok(AppSettingsDto::default());
    }
    let content = std::fs::read_to_string(path).map_err(CommandError::from_typed_service_error)?;
    let settings: AppSettingsDto =
        serde_json::from_str(&content).map_err(CommandError::from_typed_service_error)?;
    settings.validate().map_err(CommandError::invalid_input)?;
    Ok(settings)
}

/// Read-only probe for the optional `capabilityLifecycle.enabled` switch on the
/// shared `app-settings.json`. Absent file, missing key, non-boolean value, or
/// parse failure all degrade to `true` (detached post-import lifecycle stays
/// on). This never writes the file and never fails the import path; the switch
/// is read once per import job, so a change only affects the next import.
pub(crate) fn read_capability_lifecycle_enabled(path: &PathBuf) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        tracing::warn!(
            "app-settings.json is not valid JSON; capability lifecycle defaults to enabled"
        );
        return true;
    };
    value
        .get("capabilityLifecycle")
        .and_then(|object| object.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(test)]
#[path = "../../tests/unit/commands/settings_commands.rs"]
mod tests;
