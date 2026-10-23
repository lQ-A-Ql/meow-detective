use domain::DataSourceId;
use serde::Deserialize;
use serde_json::{json, Value};
use transport::commands::{
    ClassifyFilesRequest, GetAnalysisExtractionRequest, GetAnalysisSourceRequest,
    GetAndroidPackagesRequest, GetEvtxEventSummaryRequest, GetPluginFamilyEntriesRequest,
};

use super::arguments::{parse_arguments, validate_arguments};
use super::{McpHostQueryContext, McpHostServiceError};
use crate::analysis_service::{AnalysisSourceReadRuntime, MAX_SAMPLE_SIZE};

const MCP_CLASSIFICATION_SAMPLE_SIZE: u32 = 300;
const CORRELATION_PAGE_DEFAULT: u32 = 50;
const CORRELATION_PAGE_MAX: u32 = 100;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorrelationSnapshotRequest {
    #[serde(default)]
    offset: u64,
    #[serde(default = "default_correlation_limit")]
    limit: u32,
}

fn default_correlation_limit() -> u32 {
    CORRELATION_PAGE_DEFAULT
}

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
        | "forensics.get_android_device_info"
        | "forensics.get_android_package_summary"
        | "forensics.get_plugin_family_entries"
        | "forensics.get_analysis_report" => execute_source_tool(&context, name, arguments)
            .map_err(|error| add_source_context(arguments, error)),
        "forensics.get_case_overview" => case_overview(&context),
        "forensics.get_correlation_snapshot" => correlation_snapshot(&context, arguments),
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

fn case_overview(context: &McpHostAnalysisQueryContext<'_>) -> Result<Value, McpHostServiceError> {
    crate::v3_governance_service::get_case_overview_snapshot_for_case_read_only(
        context.query.connection,
        context.query.case_root,
        &context.query.case_meta.id.0,
    )
    .map_err(|_| McpHostServiceError::Overview)
    .and_then(|value| serde_json::to_value(value).map_err(McpHostServiceError::from))
}

fn correlation_snapshot(
    context: &McpHostAnalysisQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let request: CorrelationSnapshotRequest = parse_arguments(arguments)?;
    if !(1..=CORRELATION_PAGE_MAX).contains(&request.limit) {
        return Err(McpHostServiceError::Invalid(format!(
            "limit 必须在 1 到 {CORRELATION_PAGE_MAX} 之间"
        )));
    }
    let offset = usize::try_from(request.offset)
        .map_err(|_| McpHostServiceError::Invalid("offset 超出平台范围".to_string()))?;
    let limit = request.limit as usize;
    let snapshot = crate::correlation::get_correlation_snapshot_read_only_for_case(
        context.query.connection,
        context.query.case_root,
        &context.query.case_meta.id,
    )
    .map_err(|_| McpHostServiceError::Correlation)?;
    let end = request.offset.saturating_add(u64::from(request.limit));
    let has_more = u64::from(snapshot.node_count) > end
        || u64::from(snapshot.edge_count) > end
        || u64::from(snapshot.cluster_count) > end
        || u64::from(snapshot.lead_count) > end;
    let nodes = take_page(snapshot.nodes, offset, limit);
    let edges = take_page(snapshot.edges, offset, limit);
    let clusters = take_page(snapshot.clusters, offset, limit);
    let leads = take_page(snapshot.leads, offset, limit);
    Ok(json!({
        "generatedAt": snapshot.generated_at,
        "nodeCount": snapshot.node_count,
        "edgeCount": snapshot.edge_count,
        "clusterCount": snapshot.cluster_count,
        "leadCount": snapshot.lead_count,
        "familyCoverage": snapshot.family_coverage,
        "offset": request.offset,
        "limit": request.limit,
        "hasMore": has_more,
        "nextOffset": has_more.then_some(end),
        "nodes": nodes,
        "edges": edges,
        "clusters": clusters,
        "leads": leads,
    }))
}

fn take_page<T>(items: Vec<T>, offset: usize, limit: usize) -> Vec<T> {
    items.into_iter().skip(offset).take(limit).collect()
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
