use serde_json::Value;

use super::{
    arguments::required_string, sanitization::source_summary, McpHostQueryContext,
    McpHostServiceError,
};

pub(super) fn list_data_sources(
    context: &McpHostQueryContext<'_>,
) -> Result<Value, McpHostServiceError> {
    let sources = load_sources(context)?;
    Ok(Value::Array(
        sources.into_iter().map(source_summary).collect(),
    ))
}

pub(super) fn get_data_source(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let id = required_string(arguments, "dataSourceId")?;
    load_sources(context)?
        .into_iter()
        .find(|source| source.id == id)
        .map(source_summary)
        .ok_or(McpHostServiceError::NotFound)
}

fn load_sources(
    context: &McpHostQueryContext<'_>,
) -> Result<Vec<transport::dto::DataSourceSummaryDto>, McpHostServiceError> {
    crate::file_service::get_data_sources_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
    )
    .map_err(McpHostServiceError::DataSource)
}
