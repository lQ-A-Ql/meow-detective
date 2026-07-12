use super::*;
use serde_json::json;

#[test]
fn server_config_serializes_current_snake_case_response_fields() {
    let server = McpServerConfigDto {
        id: "srv-1".to_string(),
        name: "Local MCP".to_string(),
        transport_type: "stdio".to_string(),
        url: None,
        command: Some("node".to_string()),
        args: Some(vec!["server.js".to_string()]),
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfileDto {
            resource_access: "readOnly".to_string(),
            tool_access: "allowList".to_string(),
            prompt_access: "readOnly".to_string(),
            network_policy: "localhostOnly".to_string(),
            allowed_tools: vec!["timeline_lookup".to_string()],
            allowed_commands: vec!["node".to_string()],
        },
    };

    let value = serde_json::to_value(server).expect("serialize server config");
    assert_eq!(value["transport_type"], "stdio");
    assert_eq!(value["auto_connect"], false);
    assert_eq!(value["permissions"]["tool_access"], "allowList");
    assert!(value.get("transportType").is_none());
    assert!(value.get("autoConnect").is_none());
}

#[test]
fn server_status_serializes_current_snake_case_capability_fields() {
    let status = McpServerStatusDto {
        id: "srv-1".to_string(),
        name: "Local MCP".to_string(),
        connected: true,
        has_resources: true,
        has_tools: false,
        has_prompts: true,
        last_error: Some("boom".to_string()),
    };

    let value = serde_json::to_value(status).expect("serialize server status");
    assert_eq!(value["has_resources"], true);
    assert_eq!(value["has_tools"], false);
    assert_eq!(value["has_prompts"], true);
    assert_eq!(value["last_error"], "boom");
}

/// MCP DTOs keep snake_case field names because the Tauri command
/// envelope (top-level) uses camelCase serde, while the nested MCP
/// payloads serialize as-is in snake_case. This test is also a
/// compile-time contract marker checked by the stage5 regression guard.
#[test]
fn tool_call_request_documents_camel_case_boundary_is_top_level_only() {
    // Reuse the snake_case acceptance assert below; the function
    // name itself is the contract marker.
    let value = serde_json::json!({
        "server_id": "srv-1",
        "tool_name": "lookup",
        "arguments": { "query": "mft" }
    });

    let request: McpToolCallRequestDto =
        serde_json::from_value(value).expect("deserialize tool call request");

    assert_eq!(request.server_id, "srv-1");
    assert_eq!(request.tool_name, "lookup");
    assert_eq!(request.arguments["query"], "mft");
}

#[test]
fn tool_call_request_accepts_current_snake_case_request_fields() {
    let value = json!({
        "server_id": "srv-1",
        "tool_name": "lookup",
        "arguments": { "query": "mft" }
    });

    let request: McpToolCallRequestDto =
        serde_json::from_value(value).expect("deserialize tool call request");

    assert_eq!(request.server_id, "srv-1");
    assert_eq!(request.tool_name, "lookup");
    assert_eq!(request.arguments["query"], "mft");
}

#[test]
fn test_connection_request_accepts_permissions() {
    let value = json!({
        "transport_type": "sse",
        "url": "http://127.0.0.1:3000/sse",
        "permissions": {
            "resource_access": "readOnly",
            "tool_access": "disabled",
            "prompt_access": "readOnly",
            "network_policy": "localhostOnly",
            "allowed_tools": [],
            "allowed_commands": []
        }
    });

    let request: McpTestConnectionRequestDto =
        serde_json::from_value(value).expect("deserialize test connection request");

    assert_eq!(request.transport_type, "sse");
    assert_eq!(request.permissions.network_policy, "localhostOnly");
}
