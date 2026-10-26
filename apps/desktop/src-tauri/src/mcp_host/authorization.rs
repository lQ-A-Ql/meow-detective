use crate::state::AppState;
use app_services::mcp_host_service::access::McpPrincipal;
use axum::{
    extract::{Request, State},
    http::{uri::Authority, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
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
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(bearer) = headers.typed_get::<Authorization<Bearer>>() else {
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer")]).into_response();
    };
    let principal = {
        let Ok(access) = state.mcp_host.access.lock() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        access.authenticate(bearer.token())
    };
    let Ok(principal) = principal else {
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer")]).into_response();
    };
    let valid_case = {
        let Ok(active) = state.active_case.lock() else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        active.as_ref().is_some_and(|active| {
            active.meta.id.0 == principal.client.case_id
                && active.session_id == principal.client.case_session_id
        })
    };
    if !valid_case {
        return StatusCode::FORBIDDEN.into_response();
    }
    request.extensions_mut().insert(principal);
    request
        .headers_mut()
        .remove(axum::http::header::AUTHORIZATION);
    next.run(request).await
}

pub(super) fn principal(context: &RequestContext<RoleServer>) -> Result<McpPrincipal, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<McpPrincipal>())
        .cloned()
        .ok_or_else(|| ErrorData::invalid_request("客户端授权不可用", None))
}
