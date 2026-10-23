use domain::{CaseId, DataSourceId};

use crate::file_service::{FileServiceError, PreviewRuntimeRegistry};

pub fn close_preview_session_for_case(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<bool, FileServiceError> {
    registry.close_session(&case_id.0, handle_id)
}

pub fn preview_session_file_id(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<String, FileServiceError> {
    registry
        .get_session(&case_id.0, handle_id)
        .map(|session| session.global_file_id().to_string())
}

pub fn preview_session_metadata(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<(u64, Option<String>), FileServiceError> {
    registry
        .get_session(&case_id.0, handle_id)
        .map(|session| (session.size(), session.mime().map(str::to_string)))
}

pub fn invalidate_preview_source(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<(), FileServiceError> {
    registry.invalidate_source(&case_id.0, &data_source_id.0)
}
