use serde::Deserialize;
use serde_json::{json, Value};
use transport::dto::{GetProvenanceChainRequest, GraphQueryDto};

use super::super::{arguments::parse_arguments, McpHostAnalysisQueryContext, McpHostServiceError};

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

pub(super) fn case_overview(
    context: &McpHostAnalysisQueryContext<'_>,
) -> Result<Value, McpHostServiceError> {
    crate::v3_governance_service::get_case_overview_snapshot_for_case_read_only(
        context.query.connection,
        context.query.case_root,
        &context.query.case_meta.id.0,
    )
    .map_err(|_| McpHostServiceError::Overview)
    .and_then(|value| serde_json::to_value(value).map_err(McpHostServiceError::from))
}

pub(super) fn correlation_snapshot(
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

pub(super) fn query_graph(
    context: &McpHostAnalysisQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let request: GraphQueryDto = parse_arguments(arguments)?;
    let result = crate::graph_service::query_graph_for_case_read_only(
        context.query.connection,
        context.query.case_root,
        &context.query.case_meta.id.0,
        request,
    )
    .map_err(|_| McpHostServiceError::Correlation)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}

pub(super) fn provenance_chain(
    context: &McpHostAnalysisQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let request: GetProvenanceChainRequest = parse_arguments(arguments)?;
    let result = crate::graph_service::get_provenance_chain_for_case_read_only(
        context.query.connection,
        context.query.case_root,
        &context.query.case_meta.id.0,
        &request.edge_id,
    )
    .map_err(|_| McpHostServiceError::Correlation)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}

fn take_page<T>(items: Vec<T>, offset: usize, limit: usize) -> Vec<T> {
    items.into_iter().skip(offset).take(limit).collect()
}
