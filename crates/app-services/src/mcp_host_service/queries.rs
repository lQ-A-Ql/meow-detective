use super::{arguments::validate_arguments, McpHostServiceError};
use domain::CaseMeta;
use rusqlite::Connection;
use serde_json::Value;
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
        "forensics.get_current_case" => Ok(super::current_case::get_current_case(&context)),
        "forensics.list_data_sources" => super::data_sources::list_data_sources(&context),
        "forensics.get_data_source" => super::data_sources::get_data_source(&context, arguments),
        "forensics.list_plugin_modules" => {
            super::plugin_modules::list_plugin_modules(&context, arguments)
        }
        "forensics.get_plugin_module" => {
            super::plugin_modules::get_plugin_module(&context, arguments)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}
