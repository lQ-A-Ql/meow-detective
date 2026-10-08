use domain::DataSourceId;
use serde_json::Value;

use super::{
    arguments::required_string, sanitization::plugin_summary, McpHostQueryContext,
    McpHostServiceError,
};

pub(super) fn list_plugin_modules(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let modules = load_modules(context, arguments)?;
    Ok(Value::Array(
        modules.into_iter().map(plugin_summary).collect(),
    ))
}

pub(super) fn get_plugin_module(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let plugin_id = required_string(arguments, "pluginId")?;
    load_modules(context, arguments)?
        .into_iter()
        .find(|module| module.plugin_id == plugin_id)
        .map(plugin_summary)
        .ok_or(McpHostServiceError::NotFound)
}

fn load_modules(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Vec<transport::dto::PluginModuleDto>, McpHostServiceError> {
    let id = DataSourceId(required_string(arguments, "dataSourceId")?.into());
    crate::analysis_service::get_source_plugin_modules(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        &id,
    )
    .map_err(McpHostServiceError::Plugin)
}
