use super::{
    sanitization::{plugin_summary, source_summary},
    tool_catalog, McpHostServiceError,
};
use domain::{CaseMeta, DataSourceId};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

pub struct McpHostQueryContext<'a> {
    pub connection: &'a Connection,
    pub case_root: &'a Path,
    pub case_meta: &'a CaseMeta,
}

pub fn execute_tool(
    context: McpHostQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    validate_arguments(name, arguments)?;
    match name {
        "forensics.get_current_case" => Ok(json!({
            "id": context.case_meta.id.0, "name": context.case_meta.name,
            "createdAt": context.case_meta.created_at,
        })),
        "forensics.list_data_sources" | "forensics.get_data_source" => {
            let sources = crate::file_service::get_data_sources_for_case(
                context.connection,
                context.case_root,
                &context.case_meta.id,
            )?;
            if name == "forensics.get_data_source" {
                let id = required_string(arguments, "dataSourceId")?;
                sources
                    .into_iter()
                    .find(|source| source.id == id)
                    .map(source_summary)
                    .ok_or(McpHostServiceError::NotFound)
            } else {
                Ok(Value::Array(
                    sources.into_iter().map(source_summary).collect(),
                ))
            }
        }
        "forensics.list_plugin_modules" | "forensics.get_plugin_module" => {
            let id = DataSourceId(required_string(arguments, "dataSourceId")?.into());
            let modules = crate::analysis_service::get_source_plugin_modules(
                context.connection,
                context.case_root,
                &context.case_meta.id,
                &id,
            )?;
            if name == "forensics.get_plugin_module" {
                let plugin_id = required_string(arguments, "pluginId")?;
                modules
                    .into_iter()
                    .find(|module| module.plugin_id == plugin_id)
                    .map(plugin_summary)
                    .ok_or(McpHostServiceError::NotFound)
            } else {
                Ok(Value::Array(
                    modules.into_iter().map(plugin_summary).collect(),
                ))
            }
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn validate_arguments(name: &str, arguments: &Value) -> Result<(), McpHostServiceError> {
    let tool = tool_catalog()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or(McpHostServiceError::InvalidInput)?;
    let fields = arguments
        .as_object()
        .ok_or(McpHostServiceError::InvalidInput)?;
    let properties = &tool.input_schema["properties"];
    if fields.keys().any(|key| properties.get(key).is_none()) {
        return Err(McpHostServiceError::InvalidInput);
    }
    for key in tool.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
    {
        required_string(
            arguments,
            key.as_str().ok_or(McpHostServiceError::InvalidInput)?,
        )?;
    }
    Ok(())
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, McpHostServiceError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .ok_or(McpHostServiceError::InvalidInput)
}
