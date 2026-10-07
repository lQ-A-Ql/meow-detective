use std::sync::Arc;

use serde_json::Value;
use transport::dto::mcp_host::{
    McpHostFileRequestDto, McpHostListFilesRequestDto, McpHostReadFileRequestDto,
};

use super::{arguments::validate_arguments, McpHostQueryContext, McpHostServiceError};
use crate::{bitlocker_runtime::BitLockerUnlockRegistry, file_service::PreviewRuntimeRegistry};

pub struct McpHostFileQueryContext<'a> {
    pub query: McpHostQueryContext<'a>,
    pub preview_runtime: &'a PreviewRuntimeRegistry,
    pub bitlocker_runtime: &'a Arc<BitLockerUnlockRegistry>,
}

pub fn execute_file_tool(
    context: McpHostFileQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    validate_arguments(name, arguments)?;
    match name {
        "forensics.list_files" => {
            let request: McpHostListFilesRequestDto = serde_json::from_value(arguments.clone())?;
            Ok(serde_json::to_value(super::list_files::list_files(
                &context.query,
                &request,
            )?)?)
        }
        "forensics.get_file_metadata" => {
            let request: McpHostFileRequestDto = serde_json::from_value(arguments.clone())?;
            let metadata = crate::file_service::get_file_metadata_for_case(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &request.file_id,
            )
            .map_err(McpHostServiceError::File)?;
            Ok(serde_json::to_value(metadata)?)
        }
        "forensics.read_file" => {
            let request: McpHostReadFileRequestDto = serde_json::from_value(arguments.clone())?;
            Ok(serde_json::to_value(super::read_file::read_file(
                &context, &request,
            )?)?)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}
