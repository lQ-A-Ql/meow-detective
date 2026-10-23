use domain::CaseId;
use serde_json::Value;

use super::arguments::required_string;
use super::{McpHostQueryContext, McpHostServiceError};

pub(super) fn get_content_index_coverage(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let data_source_id = required_string(arguments, "dataSourceId")?;
    let result = crate::content_index_service::coverage::get(
        context.connection,
        context.case_root,
        &CaseId(context.case_meta.id.0.clone()),
        data_source_id,
    )
    .map_err(|_| McpHostServiceError::ContentIndex)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}
