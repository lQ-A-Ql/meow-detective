//! `plugins.*` application settings (design doc §5.7).
//!
//! The loader reads the same `app-settings.json` the desktop shell persists
//! (`<config_dir>/Meow_Detective/app-settings.json`, see `AppState`), but only
//! the `plugins` object, and tolerates its absence so older settings files
//! keep working. Defaults: `plugins.enabled = true`, `plugins.dir` unset
//! (executable-adjacent discovery).

use std::path::{Path, PathBuf};

/// Must match `APP_CODE_NAME` in the desktop shell's `AppState`.
const APP_CODE_NAME: &str = "Meow_Detective";
const SETTINGS_FILE_NAME: &str = "app-settings.json";

/// Plugin loading toggles from `app-settings.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSettings {
    /// Master switch; `false` disables plugin discovery entirely.
    pub enabled: bool,
    /// `plugins.dir` override. `None` = executable-adjacent `plugins/`.
    pub dir: Option<PathBuf>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: None,
        }
    }
}

/// Load plugin settings from the default app-settings location. Any read or
/// parse problem degrades to defaults (plugins enabled, exe-adjacent dir).
pub fn load_plugin_settings() -> PluginSettings {
    match settings_file_path() {
        Some(path) => load_plugin_settings_from(&path),
        None => PluginSettings::default(),
    }
}

fn settings_file_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join(APP_CODE_NAME)
            .join(SETTINGS_FILE_NAME),
    )
}

/// Parse plugin settings from an explicit settings file. Exposed for tests;
/// the production entry point is [`load_plugin_settings`].
pub fn load_plugin_settings_from(path: &Path) -> PluginSettings {
    let Ok(content) = std::fs::read_to_string(path) else {
        return PluginSettings::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        tracing::warn!("app-settings.json is not valid JSON; plugin defaults apply");
        return PluginSettings::default();
    };
    let Some(plugins) = value.get("plugins") else {
        return PluginSettings::default();
    };
    let enabled = plugins
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let dir = plugins
        .get("dir")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    PluginSettings { enabled, dir }
}
