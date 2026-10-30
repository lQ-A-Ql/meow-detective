use domain::DataSourceId;
use serde_json::Value;
use transport::commands::{
    ClassifyFilesRequest, GetAnalysisExtractionRequest, GetAnalysisSourceRequest,
    GetAndroidPackagesRequest, GetEvtxEventSummaryRequest, GetPluginFamilyEntriesRequest,
};

use super::arguments::{parse_arguments, validate_arguments};
use super::{McpHostQueryContext, McpHostServiceError};
use crate::analysis_service::{AnalysisSourceReadRuntime, MAX_SAMPLE_SIZE};
mod case;

use case::{case_overview, correlation_snapshot, provenance_chain, query_graph};

const MCP_CLASSIFICATION_SAMPLE_SIZE: u32 = 300;

pub struct McpHostAnalysisQueryContext<'a> {
    pub query: McpHostQueryContext<'a>,
    pub runtime: &'a AnalysisSourceReadRuntime,
}

pub fn execute_analysis_tool(
    context: McpHostAnalysisQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    validate_arguments(name, arguments)?;
    match name {
        "forensics.get_analysis_system_info"
        | "forensics.get_evidence_classification_summary"
        | "forensics.get_file_classification_board"
        | "forensics.get_registry_summary"
        | "forensics.get_registry_structured_summary"
        | "forensics.get_browser_history_summary"
        | "forensics.get_email_summary"
        | "forensics.get_event_log_events"
        | "forensics.get_linux_artifact_summary"
        | "forensics.list_linux_services"
        | "forensics.list_docker_containers"
        | "forensics.list_docker_images"
        | "forensics.get_android_device_info"
        | "forensics.get_android_package_summary"
        | "forensics.get_plugin_family_entries"
        | "forensics.get_analysis_report" => execute_source_tool(&context, name, arguments)
            .map_err(|error| add_source_context(arguments, error)),
        "forensics.get_case_overview" => case_overview(&context),
        "forensics.get_correlation_snapshot" => correlation_snapshot(&context, arguments),
        "forensics.query_graph" => query_graph(&context, arguments),
        "forensics.get_provenance_chain" => provenance_chain(&context, arguments),
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn execute_source_tool(
    context: &McpHostAnalysisQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    match name {
        "forensics.get_analysis_system_info"
        | "forensics.get_evidence_classification_summary"
        | "forensics.get_file_classification_board"
        | "forensics.get_registry_structured_summary"
        | "forensics.get_analysis_report" => execute_summary_tool(context, name, arguments),
        "forensics.get_registry_summary"
        | "forensics.get_browser_history_summary"
        | "forensics.get_email_summary"
        | "forensics.get_event_log_events"
        | "forensics.get_linux_artifact_summary" => execute_paged_tool(context, name, arguments),
        "forensics.get_android_device_info"
        | "forensics.get_android_package_summary"
        | "forensics.get_plugin_family_entries" => {
            execute_source_detail_tool(context, name, arguments)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn execute_summary_tool(
    context: &McpHostAnalysisQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    match name {
        "forensics.get_analysis_system_info" => {
            let request = source_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_system_info(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                context.runtime,
            );
            to_value(result)
        }
        "forensics.get_evidence_classification_summary" => {
            let request = source_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_evidence_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
            );
            to_value(result)
        }
        "forensics.get_file_classification_board" => {
            let request = classification_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let sample_size = request
                .sample_size
                .unwrap_or(MCP_CLASSIFICATION_SAMPLE_SIZE);
            let result = crate::analysis_service::get_file_classification_board(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                sample_size,
                context.runtime,
            );
            to_value(result)
        }
        "forensics.get_registry_structured_summary" => {
            let request = source_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_registry_structured_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
            );
            to_value(result)
        }
        "forensics.get_analysis_report" => {
            let request = source_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::generate_source_analysis_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                context.runtime,
            );
            to_value(result)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn execute_paged_tool(
    context: &McpHostAnalysisQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    match name {
        "forensics.get_registry_summary" => {
            let request = extraction_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_registry_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        "forensics.get_browser_history_summary" => {
            let request = extraction_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_browser_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        "forensics.get_email_summary" => {
            let request = extraction_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_email_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        "forensics.get_event_log_events" => {
            let request = evtx_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_evtx_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.view,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        "forensics.get_linux_artifact_summary" => {
            let request = extraction_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_linux_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        "forensics.list_linux_services"
        | "forensics.list_docker_containers"
        | "forensics.list_docker_images" => {
            let request = extraction_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let config_kind = match name {
                "forensics.list_linux_services" => "systemdServiceUnit",
                "forensics.list_docker_containers" => "dockerContainer",
                "forensics.list_docker_images" => "dockerImage",
                _ => unreachable!(),
            };
            let result = crate::analysis_service::get_source_linux_system_config_page(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                config_kind,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn execute_source_detail_tool(
    context: &McpHostAnalysisQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    match name {
        "forensics.get_android_device_info" => {
            let request = source_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_android_device_info(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
            );
            to_value(result)
        }
        "forensics.get_android_package_summary" => {
            let request = android_packages_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_android_package_summary(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                request.offset,
                request.limit,
                context.runtime,
            );
            to_value(result)
        }
        "forensics.get_plugin_family_entries" => {
            let request = plugin_family_request(arguments)?;
            let source_id = DataSourceId(request.data_source_id);
            let result = crate::analysis_service::get_source_plugin_family_entries(
                context.query.connection,
                context.query.case_root,
                &context.query.case_meta.id,
                &source_id,
                &request.plugin_id,
                &request.family,
                request.offset,
                request.limit,
            );
            to_value(result)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn source_request(arguments: &Value) -> Result<GetAnalysisSourceRequest, McpHostServiceError> {
    let request: GetAnalysisSourceRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    Ok(request)
}

fn extraction_request(
    arguments: &Value,
) -> Result<GetAnalysisExtractionRequest, McpHostServiceError> {
    let mut request: GetAnalysisExtractionRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    Ok(request)
}

fn evtx_request(arguments: &Value) -> Result<GetEvtxEventSummaryRequest, McpHostServiceError> {
    let mut request: GetEvtxEventSummaryRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    Ok(request)
}

fn android_packages_request(
    arguments: &Value,
) -> Result<GetAndroidPackagesRequest, McpHostServiceError> {
    let mut request: GetAndroidPackagesRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    Ok(request)
}

fn plugin_family_request(
    arguments: &Value,
) -> Result<GetPluginFamilyEntriesRequest, McpHostServiceError> {
    let mut request: GetPluginFamilyEntriesRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    Ok(request)
}

fn classification_request(arguments: &Value) -> Result<ClassifyFilesRequest, McpHostServiceError> {
    let request: ClassifyFilesRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    let sample_size = request
        .sample_size
        .unwrap_or(MCP_CLASSIFICATION_SAMPLE_SIZE);
    if sample_size == 0 || sample_size > MAX_SAMPLE_SIZE {
        return Err(McpHostServiceError::InvalidInput);
    }
    Ok(request)
}

fn to_value<T: serde::Serialize>(
    result: Result<T, crate::analysis_service::AnalysisServiceError>,
) -> Result<Value, McpHostServiceError> {
    let value = result.map_err(analysis_error)?;
    serde_json::to_value(value).map_err(McpHostServiceError::from)
}

fn analysis_error(error: crate::analysis_service::AnalysisServiceError) -> McpHostServiceError {
    use crate::analysis_service::AnalysisServiceError;
    match error {
        AnalysisServiceError::Unsupported(message) => {
            McpHostServiceError::UnsupportedPlatform(message)
        }
        AnalysisServiceError::NotFound(kind, id) => {
            McpHostServiceError::NotFoundDetail(format!("{kind} {id}"))
        }
        AnalysisServiceError::InvalidInput(message) => McpHostServiceError::Invalid(message),
        error => McpHostServiceError::Analysis(error),
    }
}

fn add_source_context(arguments: &Value, error: McpHostServiceError) -> McpHostServiceError {
    let Some(source_id) = arguments
        .get("dataSourceId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return error;
    };
    if matches!(
        &error,
        McpHostServiceError::Invalid(_) | McpHostServiceError::InvalidInput
    ) || error.to_string().contains(source_id)
    {
        return error;
    }
    McpHostServiceError::SourceRead {
        data_source_id: source_id.to_string(),
        source: Box::new(error),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp_host_service/analysis.rs"]
mod tests;
