use std::{path::PathBuf, sync::Arc};

use serde_json::Value;
use transport::dto::mcp_host::{
    McpHostExtractFileRequestDto, McpHostFileRequestDto, McpHostListFilesRequestDto,
    McpHostReadFileRequestDto, McpHostRegistryBrowseRequestDto,
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
            .map_err(|source| McpHostServiceError::FileRead {
                file_id: request.file_id.clone(),
                source,
            })?;
            Ok(serde_json::to_value(metadata)?)
        }
        "forensics.browse_registry_key" => {
            let request: McpHostRegistryBrowseRequestDto =
                serde_json::from_value(arguments.clone())?;
            let result = crate::file_service::browse_registry_key_for_case(
                context.bitlocker_runtime,
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &request.file_id,
                &request.key_path,
            )
            .map_err(|source| McpHostServiceError::FileRead {
                file_id: request.file_id.clone(),
                source,
            })?;
            Ok(serde_json::to_value(result)?)
        }
        "forensics.read_file" => {
            let request: McpHostReadFileRequestDto = serde_json::from_value(arguments.clone())?;
            Ok(serde_json::to_value(super::read_file::read_file(
                &context, &request,
            )?)?)
        }
        "forensics.extract_file" => {
            let request: McpHostExtractFileRequestDto = serde_json::from_value(arguments.clone())?;
            request.validate().map_err(McpHostServiceError::Invalid)?;
            let destination_path = request
                .destination_path
                .as_deref()
                .ok_or_else(|| McpHostServiceError::Invalid("导出目录授权尚未解析".into()))?;
            let destination = PathBuf::from(destination_path);
            let mut progress = |_| {};
            let result = crate::file_service::extract_file_for_case_with_audit(
                context.bitlocker_runtime,
                crate::file_service::CaseFileExtractionRequest {
                    case_conn: context.query.connection,
                    case_root: context.query.case_root,
                    case_id: &context.query.case_meta.id,
                    file_id: &request.file_id,
                    destination_path: &destination,
                    overwrite: request.overwrite,
                },
                &mut progress,
            )
            .map_err(|error| McpHostServiceError::Extraction(error.message))?;
            Ok(serde_json::to_value(result)?)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}
