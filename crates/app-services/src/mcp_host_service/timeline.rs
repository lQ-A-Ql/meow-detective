use serde_json::Value;
use transport::commands::{
    GetTimelineEventByIdRequest, GetTimelineFacetsRequest, GetTimelineRequest,
};

use super::arguments::parse_arguments;
use super::{McpHostQueryContext, McpHostServiceError};
use crate::timeline_service::TimelineQuery;

pub(super) fn execute_timeline_tool(
    context: &McpHostQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    match name {
        "forensics.get_timeline_events" => timeline_events(context, arguments),
        "forensics.get_timeline_event" => timeline_event(context, arguments),
        "forensics.get_timeline_facets" => timeline_facets(context, arguments),
        _ => Err(McpHostServiceError::InvalidInput),
    }
}

fn timeline_events(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let mut request: GetTimelineRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    let query = TimelineQuery {
        offset: request.offset,
        limit: request.limit,
        time_start: request.time_start.as_deref(),
        time_end: request.time_end.as_deref(),
        event_type: request.event_type.as_deref(),
        cursor: request.cursor.as_deref(),
    };
    let result = crate::timeline_service::query_timeline_filtered_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        query,
    )
    .map_err(McpHostServiceError::Timeline)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}

fn timeline_event(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let request: GetTimelineEventByIdRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    let result = crate::timeline_service::get_timeline_event_by_id_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        &request.event_id,
    )
    .map_err(|source| McpHostServiceError::TimelineRead {
        event_id: request.event_id.clone(),
        source,
    })?
    .ok_or_else(|| {
        McpHostServiceError::NotFoundDetail(format!("时间线事件 {}", request.event_id))
    })?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}

fn timeline_facets(
    context: &McpHostQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let mut request: GetTimelineFacetsRequest = parse_arguments(arguments)?;
    request
        .validate()
        .map_err(|_| McpHostServiceError::InvalidInput)?;
    let result = crate::timeline_service::get_timeline_facets_for_case(
        context.connection,
        context.case_root,
        &context.case_meta.id,
        request.time_start.as_deref(),
        request.time_end.as_deref(),
        request.event_type.as_deref(),
        request.bucket_count,
    )
    .map_err(McpHostServiceError::Timeline)?;
    serde_json::to_value(result).map_err(McpHostServiceError::from)
}
