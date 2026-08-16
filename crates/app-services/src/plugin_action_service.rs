//! Generic plugin action channel service (ABI doc §3 optional export).
//!
//! Resolves a loaded plugin by id and drives its self-describing JSON
//! action channel: `list_plugin_actions` powers the
//! `list_plugin_actions` Tauri command; `call_plugin_action` is the
//! crate-internal entry used by concrete action consumers (e.g.
//! `wechat_key_service`).

use crate::plugin_loader::{self, PluginExtractor};
use artifacts_core::ArtifactExtractor;
use serde_json::Value;
use transport::dto::PluginActionDescriptorDto;
use transport::{ErrorCategory, ServiceErrorCategory};

/// Errors of the plugin action channel and its consumers.
#[derive(Debug, thiserror::Error)]
pub enum PluginActionError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The plugin reported a failure through the action channel. The
    /// message is plugin-sourced; action contracts forbid secrets and
    /// sensitive host paths in it.
    #[error("plugin action failed: {0}")]
    Plugin(String),
}

impl ServiceErrorCategory for PluginActionError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) => ErrorCategory::Io,
            Self::NotFound(_, _) | Self::InvalidInput(_) => ErrorCategory::Validation,
            Self::Unsupported(_) => ErrorCategory::Unsupported,
            Self::Plugin(_) => ErrorCategory::External,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        matches!(self, Self::Plugin(_) | Self::Unsupported(_)).then_some(true)
    }
}

impl From<crate::source_db::ReadySourceError> for PluginActionError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { data_source_id, .. } => {
                Self::NotFound("Data source", data_source_id)
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}

/// The plugin's self-described action list (empty when the plugin does not
/// export the action channel — graceful degradation, never an error).
pub fn list_plugin_actions(
    plugin_id: &str,
) -> Result<Vec<PluginActionDescriptorDto>, PluginActionError> {
    if plugin_id.trim().is_empty() {
        return Err(PluginActionError::InvalidInput(
            "pluginId must not be blank".to_string(),
        ));
    }
    let plugin = find_plugin(plugin_id)?;
    let value = plugin
        .describe_actions()
        .map_err(PluginActionError::Plugin)?;
    let actions = value.as_array().cloned().unwrap_or_default();
    Ok(actions.iter().filter_map(parse_descriptor).collect())
}

/// Invoke one action on a loaded plugin (crate-internal consumers only;
/// Tauri commands never expose a generic action invocation).
pub(crate) fn call_plugin_action(
    plugin_id: &str,
    action: &str,
    params: &Value,
) -> Result<Value, PluginActionError> {
    let plugin = find_plugin(plugin_id)?;
    if !plugin.has_actions() {
        return Err(PluginActionError::Unsupported(format!(
            "plugin {plugin_id} does not export the action channel"
        )));
    }
    plugin
        .call_action(action, params)
        .map_err(PluginActionError::Plugin)
}

fn find_plugin(plugin_id: &str) -> Result<PluginExtractor, PluginActionError> {
    plugin_loader::load_all_report()
        .plugins
        .into_iter()
        .find(|plugin| plugin.id() == plugin_id)
        .ok_or_else(|| PluginActionError::NotFound("Plugin", plugin_id.to_string()))
}

/// Parse one `describe` element tolerantly: entries without `id`/`label`
/// strings are dropped rather than failing the whole listing.
fn parse_descriptor(value: &Value) -> Option<PluginActionDescriptorDto> {
    let id = value.get("id")?.as_str()?.to_string();
    let label = value.get("label")?.as_str()?.to_string();
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let input_kind = value
        .get("inputKind")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    Some(PluginActionDescriptorDto {
        id,
        label,
        description,
        input_kind,
    })
}

#[cfg(test)]
#[path = "../tests/unit/plugin_action_service.rs"]
mod tests;
