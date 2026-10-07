use domain::DataSourceId;
use transport::dto::{mcp_host::McpHostListFilesRequestDto, FileRowsPageDto};

use super::{McpHostQueryContext, McpHostServiceError};

pub(super) fn list_files(
    context: &McpHostQueryContext<'_>,
    request: &McpHostListFilesRequestDto,
) -> Result<FileRowsPageDto, McpHostServiceError> {
    crate::file_service::list_file_entries_page_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        &DataSourceId(request.data_source_id.clone()),
        request.parent_id.as_deref(),
        request.offset,
        request.limit,
    )
    .map_err(McpHostServiceError::File)
}
