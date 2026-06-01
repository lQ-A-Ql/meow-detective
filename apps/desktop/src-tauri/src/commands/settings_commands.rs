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
            std::fs::create_dir_all(parent).map_err(CommandError::from_service_error)?;
        }
        let content =
            serde_json::to_string_pretty(&settings).map_err(CommandError::from_service_error)?;
        std::fs::write(&path, content).map_err(CommandError::from_service_error)?;
        Ok(settings)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn load_app_settings(path: &PathBuf) -> Result<AppSettingsDto, CommandError> {
    if !path.exists() {
        return Ok(AppSettingsDto::default());
    }
    let content = std::fs::read_to_string(path).map_err(CommandError::from_service_error)?;
    let settings: AppSettingsDto =
        serde_json::from_str(&content).map_err(CommandError::from_service_error)?;
    settings.validate().map_err(CommandError::invalid_input)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_app_settings_returns_default_when_file_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let settings = load_app_settings(&temp.path().join("missing.json")).unwrap();

        assert_eq!(settings.theme, "light");
        assert!(!settings.case_root.trim().is_empty());
    }

    #[test]
    fn persisted_app_settings_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let case_root = temp.path().join("cases");
        let search_root = temp.path().join("images");
        std::fs::create_dir_all(&case_root).unwrap();
        std::fs::create_dir_all(&search_root).unwrap();
        let path = temp.path().join("settings.json");
        let settings = AppSettingsDto {
            case_root: case_root.display().to_string(),
            image_search_paths: vec![search_root.display().to_string()],
            theme: "dark".to_string(),
            dev_event_trace: true,
        };

        std::fs::write(&path, serde_json::to_string(&settings).unwrap()).unwrap();
        let loaded = load_app_settings(&path).unwrap();

        assert_eq!(loaded.case_root, settings.case_root);
        assert_eq!(loaded.image_search_paths, settings.image_search_paths);
        assert_eq!(loaded.theme, "dark");
        assert!(loaded.dev_event_trace);
    }
}
