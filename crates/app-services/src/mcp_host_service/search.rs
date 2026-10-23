use serde_json::Value;
use transport::commands::SearchFilesRequest;

use super::arguments::parse_arguments;
use super::{McpHostQueryContext, McpHostServiceError};

pub(super) fn execute_search_tool(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let mut request: SearchFilesRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    let result = crate::search_service::search_files_request_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        &request,
        |_| {},
    )
    .map_err(McpHostServiceError::Search)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}
