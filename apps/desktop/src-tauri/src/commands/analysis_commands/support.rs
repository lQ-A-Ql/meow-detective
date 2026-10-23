use app_services::analysis_service;
use transport::{commands::ClassifyFilesRequest, commands::GetAnalysisSourceRequest, CommandError};

pub(super) use crate::commands::command_support::run_case_read_command as run_read_case_command;

pub(super) fn resolve_sample_size(request: &ClassifyFilesRequest) -> Result<u32, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let sample_size = request
        .sample_size
        .unwrap_or(analysis_service::DEFAULT_SAMPLE_SIZE);
    if sample_size == 0 || sample_size > analysis_service::MAX_SAMPLE_SIZE {
        return Err(CommandError::invalid_input(format!(
            "sampleSize must be between 1 and {}",
            analysis_service::MAX_SAMPLE_SIZE
        )));
    }
    Ok(sample_size)
}

pub(super) fn validate_source_request(
    request: &GetAnalysisSourceRequest,
) -> Result<(), CommandError> {
    request.validate().map_err(CommandError::invalid_input)
}
