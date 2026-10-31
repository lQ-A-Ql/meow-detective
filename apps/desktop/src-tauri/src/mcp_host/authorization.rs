use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::{uri::Authority, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rmcp::{
    service::{RequestContext, RoleServer},
    ErrorData,
};

pub(super) async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<Authority>().ok());
    if headers.contains_key("origin")
        || !host.is_some_and(|h| matches!(h.host(), "127.0.0.1" | "localhost"))
    {
        return deny(StatusCode::FORBIDDEN, "MCP_HTTP_SOURCE_REJECTED");
    }
    let principal = {
        let Ok(active) = state.active_case.lock() else {
            return deny(
                StatusCode::SERVICE_UNAVAILABLE,
                "MCP_CASE_STATE_UNAVAILABLE",
            );
        };
        let Some(active) = active.as_ref() else {
            return deny(StatusCode::FORBIDDEN, "MCP_CASE_REQUIRED");
        };
        app_services::mcp_host_service::access::McpPrincipal::open_case(
            active.meta.id.0.clone(),
            active.session_id.clone(),
        )
    };
    let principal_id = principal.client.id.clone();
    request.extensions_mut().insert(principal);
    let response = next.run(request).await;
    if response.status().is_client_error() || response.status().is_server_error() {
        tracing::info!(
            action = "mcp.http.reject",
            status = response.status().as_u16(),
            principal_id,
            code = "MCP_PROTOCOL_REJECTED",
            "MCP HTTP audit"
        );
    }
    response
}

pub(super) fn principal(
    context: &RequestContext<RoleServer>,
) -> Result<app_services::mcp_host_service::access::McpPrincipal, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| {
            parts
                .extensions
                .get::<app_services::mcp_host_service::access::McpPrincipal>()
        })
        .cloned()
        .ok_or_else(|| ErrorData::invalid_request("客户端授权不可用", None))
}

fn deny(status: StatusCode, code: &'static str) -> Response {
    tracing::info!(
        action = "mcp.http.reject",
        status = status.as_u16(),
        code,
        "MCP HTTP audit"
    );
    if status == StatusCode::UNAUTHORIZED {
        (status, [("www-authenticate", "Bearer")]).into_response()
    } else {
        status.into_response()
    }
}
