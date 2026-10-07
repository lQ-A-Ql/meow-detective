use super::*;
use crate::state::AppState;
use serde_json::{json, Value};
use transport::dto::mcp_host::McpHostToolCallRequestDto;

mod files;

async fn server() -> (AppState, String, tempfile::TempDir) {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState {
        mcp_config_path: temporary.path().join("mcp-config.json"),
        ..Default::default()
    };
    state
        .mcp_host
        .start_on(state.clone(), "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let endpoint = state.mcp_host.status().unwrap().endpoint;
    (state, endpoint, temporary)
}

async fn request(endpoint: &str, value: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(endpoint)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-03-26")
        .json(&value)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn real_http_handshake_discovery_call_policy_and_shutdown() {
    let (state, endpoint, temporary) = server().await;
    let response = request(&endpoint, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"regression","version":"1"}}})).await;
    assert_eq!(response.status(), 200);
    assert!(response.headers().get("MCP-Session-Id").is_none());
    let result: Value = response.json().await.unwrap();
    assert_eq!(result["result"]["serverInfo"]["name"], "Meow_Detective");
    assert_eq!(result["result"]["protocolVersion"], "2025-03-26");
    let notification = request(
        &endpoint,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(notification.status(), 202);
    let result: Value = request(
        &endpoint,
        json!({"jsonrpc":"2.0","id":"tool-list","method":"tools/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(result["id"], "tool-list");
    assert_eq!(result["result"]["tools"].as_array().unwrap().len(), 8);
    assert_eq!(
        result["result"]["tools"][0]["annotations"]["readOnlyHint"],
        true
    );

    let active =
        app_services::case_service::create_case(temporary.path(), "MCP Case", None).unwrap();
    *state.active_case.lock().unwrap() = Some(active);
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"forensics.get_current_case","arguments":{}}});
    let result: Value = request(&endpoint, call.clone()).await.json().await.unwrap();
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(result["result"]["structuredContent"]["name"], "MCP Case");
    let settings = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec!["forensics.get_current_case".into()],
    };
    state
        .mcp_host
        .configure(state.clone(), settings)
        .await
        .unwrap();
    let result: Value = request(&endpoint, call).await.json().await.unwrap();
    assert_eq!(result["result"]["isError"], true);
    let stored: Value =
        serde_json::from_slice(&std::fs::read(temporary.path().join("mcp-host.json")).unwrap())
            .unwrap();
    assert_eq!(stored["disabledTools"][0], "forensics.get_current_case");
    let conn = state.get_connection().unwrap();
    let count = persistence_sqlite::repositories::audit_repo::AuditRepo::new(&conn)
        .count_by_action("mcp.tool.call")
        .unwrap();
    assert_eq!(count, 2);
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Second Case", None).unwrap(),
    );
    let result: Value = request(&endpoint, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"forensics.list_data_sources","arguments":{}}})).await.json().await.unwrap();
    assert_eq!(result["result"]["structuredContent"], json!([]));
    state.mcp_host.shutdown();
    tokio::task::yield_now().await;
    assert!(!state.mcp_host.status().unwrap().running);
    assert!(reqwest::Client::new()
        .post(endpoint)
        .json(&json!({}))
        .send()
        .await
        .is_err());
}

#[tokio::test]
async fn persisted_stop_loads_without_listening_and_corrupt_settings_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState {
        mcp_config_path: temporary.path().join("mcp-config.json"),
        ..Default::default()
    };
    let settings = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: false,
        disabled_tools: vec!["forensics.get_current_case".into()],
    };
    state
        .mcp_host
        .configure(state.clone(), settings.clone())
        .await
        .unwrap();
    let reloaded = AppState {
        mcp_config_path: state.mcp_config_path.clone(),
        ..Default::default()
    };
    reloaded.mcp_host.initialize(reloaded.clone()).await;
    assert_eq!(reloaded.mcp_host.status().unwrap().settings, settings);
    assert!(!reloaded.mcp_host.status().unwrap().running);
    std::fs::write(temporary.path().join("mcp-host.json"), b"invalid-json").unwrap();
    reloaded.mcp_host.initialize(reloaded.clone()).await;
    assert!(!reloaded.mcp_host.status().unwrap().settings.enabled);
    assert!(reloaded.mcp_host.status().unwrap().last_error.is_some());
    std::fs::write(
        temporary.path().join("mcp-host.json"),
        vec![b' '; 33 * 1024],
    )
    .unwrap();
    reloaded.mcp_host.initialize(reloaded.clone()).await;
    assert!(reloaded
        .mcp_host
        .status()
        .unwrap()
        .last_error
        .unwrap()
        .contains("大小限制"));
}

#[tokio::test]
async fn occupied_port_and_persistence_failure_leave_the_listener_and_policy_consistent() {
    let (mut state, _endpoint, temporary) = server().await;
    let bound = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let other = AppState::default();
    assert!(other
        .mcp_host
        .start_on(other.clone(), bound.local_addr().unwrap())
        .await
        .is_err());
    assert!(!other.mcp_host.status().unwrap().running);
    let blocker = temporary.path().join("not-a-directory");
    std::fs::write(&blocker, b"blocker").unwrap();
    state.mcp_config_path = blocker.join("config.json");
    let before = state.mcp_host.status().unwrap().settings;
    let changed = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec!["forensics.get_current_case".into()],
    };
    assert!(state
        .mcp_host
        .configure(state.clone(), changed)
        .await
        .is_err());
    assert_eq!(state.mcp_host.status().unwrap().settings, before);
    assert!(state.mcp_host.status().unwrap().running);
    state.mcp_host.shutdown();
}

#[tokio::test]
async fn localhost_server_rejects_browser_origins_wrong_hosts_and_oversized_bodies() {
    let (state, endpoint, _temporary) = server().await;
    let client = reqwest::Client::new();
    for (header, value) in [
        ("Origin", "https://evil.example"),
        ("Host", "evil.example:3001"),
    ] {
        let response = client
            .post(&endpoint)
            .header(header, value)
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    let response = client
        .post(&endpoint)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .body("x".repeat(65 * 1024))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    state.mcp_host.shutdown();
}

#[tokio::test]
async fn tools_require_active_case_and_disabled_calls_cannot_bypass_the_ui() {
    let state = AppState::default();
    let request = McpHostToolCallRequestDto {
        name: "forensics.get_current_case".into(),
        arguments: json!({}),
    };
    let result = call_tool(state, request).await.unwrap();
    assert!(!result.success);
    assert!(result.error.unwrap().contains("打开案件"));
}
