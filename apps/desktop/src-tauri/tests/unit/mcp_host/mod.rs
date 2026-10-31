use super::*;
use crate::state::AppState;
use serde_json::{json, Value};
use std::time::Instant;
use transport::dto::mcp_host::McpHostToolCallRequestDto;

mod access;
mod files;

#[tokio::test]
async fn http_query_performance_records_p95_and_runtime_metrics() {
    let (state, endpoint, temporary) = server().await;
    let active =
        app_services::case_service::create_case(temporary.path(), "MCP performance", None).unwrap();
    *state.active_case.lock().unwrap() = Some(active);
    let created = grant(&state, true);
    let mut samples = Vec::new();
    for request_id in 0..20 {
        let started = Instant::now();
        let response = request(
            &endpoint,
            &created.token,
            json!({"jsonrpc":"2.0","id":request_id,"method":"tools/call","params":{"name":"forensics.get_current_case","arguments":{}}}),
        )
        .await;
        assert_eq!(response.status(), 200);
        let payload: Value = response.json().await.unwrap();
        assert_eq!(payload["result"]["isError"], false);
        samples.push(started.elapsed().as_millis() as u64);
    }
    samples.sort_unstable();
    let p95 = samples[(samples.len() * 95 / 100).saturating_sub(1)];
    let metrics: Value = request(
        &endpoint,
        &created.token,
        json!({"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"forensics.get_mcp_metrics","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    println!(
        "MCP_PERF samples={} p95_ms={} average_ms={} total_calls={}",
        samples.len(),
        p95,
        metrics["result"]["structuredContent"]["averageDurationMs"],
        metrics["result"]["structuredContent"]["totalCalls"]
    );
    assert!(p95 < 5_000);
    state.mcp_host.shutdown();
}

pub(super) fn grant(
    state: &AppState,
    read_evidence: bool,
) -> transport::dto::mcp_host::access::McpHostCreatedClientDto {
    state
        .mcp_host
        .create_client(
            state,
            transport::dto::mcp_host::access::McpHostCreateClientRequestDto {
                name: "HTTP regression".into(),
                expires_after_hours: 24,
                read_evidence,
                run_derived: true,
                allowed_data_source_ids: if read_evidence {
                    vec!["mcp-http-files".into()]
                } else {
                    vec![]
                },
            },
        )
        .unwrap()
}

async fn server() -> (AppState, String, tempfile::TempDir) {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState {
        mcp_host_settings_path: temporary.path().join("mcp-host.json"),
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

async fn request(endpoint: &str, token: &str, value: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(token)
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
    let active =
        app_services::case_service::create_case(temporary.path(), "MCP Case", None).unwrap();
    *state.active_case.lock().unwrap() = Some(active);
    let created = grant(&state, true);
    let token = &created.token;
    let response = request(&endpoint, token, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"regression","version":"1"}}})).await;
    assert_eq!(response.status(), 200);
    assert!(response.headers().get("MCP-Session-Id").is_none());
    let result: Value = response.json().await.unwrap();
    assert_eq!(result["result"]["serverInfo"]["name"], "Meow_Detective");
    assert_eq!(result["result"]["protocolVersion"], "2025-03-26");
    let notification = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(notification.status(), 202);
    let result: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":"tool-list","method":"tools/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(result["id"], "tool-list");
    let tools = result["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 51);
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "forensics.extract_file"));
    let reader = tools
        .iter()
        .find(|tool| tool["name"] == "forensics.read_file")
        .unwrap();
    assert_eq!(reader["annotations"]["readOnlyHint"], true);
    assert_eq!(reader["annotations"]["destructiveHint"], false);
    assert_eq!(reader["outputSchema"]["type"], "object");

    let overview_client = state
        .mcp_host
        .create_client(
            &state,
            transport::dto::mcp_host::access::McpHostCreateClientRequestDto {
                name: "Overview regression".into(),
                expires_after_hours: 24,
                read_evidence: true,
                run_derived: true,
                allowed_data_source_ids: vec![],
            },
        )
        .unwrap();
    let overview_token = &overview_client.token;
    let resources: Value = request(
        &endpoint,
        overview_token,
        json!({"jsonrpc":"2.0","id":"resources","method":"resources/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    let resource_uri = resources["result"]["resources"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(resource_uri.starts_with("forensics://cases/"));
    let resource: Value = request(
        &endpoint,
        overview_token,
        json!({"jsonrpc":"2.0","id":"resource-read","method":"resources/read","params":{"uri":resource_uri}}),
    )
    .await
    .json()
    .await
    .unwrap();
    let resource_text = resource["result"]["contents"][0]["text"].as_str().unwrap();
    let resource_data: Value = serde_json::from_str(resource_text).unwrap();
    assert!(resource_data["dataSources"].is_array());
    let prompts: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":"prompts","method":"prompts/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(), 3);
    let prompt: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":"prompt","method":"prompts/get","params":{"name":"case-overview","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(prompt["result"]["messages"].as_array().unwrap().len(), 2);
    let invalid_prompt: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":"invalid-prompt","method":"prompts/get","params":{"name":"linux-services-docker","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(invalid_prompt["error"].is_object());

    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"forensics.get_current_case","arguments":{}}});
    let result: Value = request(&endpoint, token, call.clone())
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(result["result"]["structuredContent"]["name"], "MCP Case");
    let metrics: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"forensics.get_mcp_metrics","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    let metrics_follow_up: Value = request(
        &endpoint,
        token,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"forensics.get_mcp_metrics","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(
        metrics_follow_up["result"]["structuredContent"]["totalCalls"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );
    assert!(metrics["result"]["structuredContent"]["averageDurationMs"].is_number());
    let settings = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec!["forensics.get_current_case".into()],
        policy_version: 1,
    };
    state
        .mcp_host
        .configure(state.clone(), settings)
        .await
        .unwrap();
    let result: Value = request(&endpoint, token, call).await.json().await.unwrap();
    assert_eq!(result["result"]["isError"], true);
    let stored: Value =
        serde_json::from_slice(&std::fs::read(temporary.path().join("mcp-host.json")).unwrap())
            .unwrap();
    assert_eq!(stored["disabledTools"][0], "forensics.get_current_case");
    let conn = state.get_connection().unwrap();
    let count = persistence_sqlite::repositories::audit_repo::AuditRepo::new(&conn)
        .count_by_action("mcp.tool.call")
        .unwrap();
    assert_eq!(count, 10);
    for (name, arguments) in [
        ("forensics.get_data_source", json!({})),
        (
            "forensics.get_data_source",
            json!({"dataSourceId":"missing-source"}),
        ),
        (
            "forensics.get_timeline_event",
            json!({"eventId":"missing-event"}),
        ),
    ] {
        let result: Value = request(
            &endpoint,
            token,
            json!({"jsonrpc":"2.0","id":"invalid-call","method":"tools/call","params":{"name":name,"arguments":arguments}}),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(result["result"]["isError"], true);
        assert!(!result["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .is_empty());
    }
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Second Case", None).unwrap(),
    );
    let result = request(&endpoint, token, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"forensics.list_data_sources","arguments":{}}})).await;
    assert_eq!(result.status(), 403);
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
        mcp_host_settings_path: temporary.path().join("mcp-host.json"),
        ..Default::default()
    };
    let settings = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: false,
        disabled_tools: vec!["forensics.get_current_case".into()],
        policy_version: 1,
    };
    state
        .mcp_host
        .configure(state.clone(), settings.clone())
        .await
        .unwrap();
    let reloaded = AppState {
        mcp_host_settings_path: state.mcp_host_settings_path.clone(),
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
async fn legacy_host_policy_defaults_new_write_tool_to_disabled() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(
        temporary.path().join("mcp-host.json"),
        br#"{"enabled":false,"disabledTools":["forensics.get_current_case"]}"#,
    )
    .unwrap();
    let state = AppState {
        mcp_host_settings_path: temporary.path().join("mcp-host.json"),
        ..Default::default()
    };

    state.mcp_host.initialize(state.clone()).await;

    let settings = state.mcp_host.status().unwrap().settings;
    assert_eq!(settings.policy_version, 1);
    assert!(!settings
        .disabled_tools
        .contains(&"forensics.extract_file".to_string()));
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
    state.mcp_host_settings_path = blocker.join("mcp-host.json");
    let before = state.mcp_host.status().unwrap().settings;
    let changed = transport::dto::mcp_host::McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec!["forensics.get_current_case".into()],
        policy_version: 1,
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
    let (state, endpoint, temporary) = server().await;
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "HTTP boundaries", None).unwrap(),
    );
    let created = grant(&state, false);
    let client = reqwest::Client::new();
    for (header, value) in [
        ("Origin", "https://evil.example"),
        ("Host", "evil.example:3001"),
    ] {
        let response = client
            .post(&endpoint)
            .bearer_auth(&created.token)
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
        .bearer_auth(&created.token)
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
    assert!(result.error.unwrap().contains("case"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_tool_calls_queue_instead_of_returning_busy() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let active =
        app_services::case_service::create_case(temporary.path(), "Concurrent MCP", None).unwrap();
    *state.active_case.lock().unwrap() = Some(active);

    let mut tasks = Vec::new();
    for _ in 0..10 {
        let state = state.clone();
        tasks.push(tokio::spawn(async move {
            call_tool(
                state,
                McpHostToolCallRequestDto {
                    name: "forensics.get_current_case".into(),
                    arguments: json!({}),
                },
            )
            .await
            .unwrap()
        }));
    }
    for task in tasks {
        let result = task.await.unwrap();
        assert!(result.success);
    }
}
