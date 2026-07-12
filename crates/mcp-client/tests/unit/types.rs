use super::*;

fn localhost_permissions() -> McpPermissionProfile {
    McpPermissionProfile::default()
}

#[test]
fn test_mcp_server_config_sse() {
    let config = McpServerConfig {
        id: "test".to_string(),
        name: "Test Server".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: false,
        permissions: localhost_permissions(),
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"Sse\""));
    assert!(json.contains("localhost:3001"));
}

#[test]
fn test_mcp_server_config_stdio() {
    let config = McpServerConfig {
        id: "test".to_string(),
        name: "Test Server".to_string(),
        transport: McpTransport::Stdio {
            command: "python".to_string(),
            args: vec!["-m".to_string(), "mcp_server".to_string()],
        },
        enabled: true,
        auto_connect: true,
        permissions: McpPermissionProfile {
            allowed_commands: vec!["python".to_string()],
            ..McpPermissionProfile::default()
        },
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"Stdio\""));
    assert!(json.contains("python"));
}

#[test]
fn test_mcp_resource() {
    let resource = McpResource {
        uri: "forensics://cases".to_string(),
        name: "Cases".to_string(),
        description: Some("List of cases".to_string()),
        mime_type: Some("application/json".to_string()),
    };

    let json = serde_json::to_string(&resource).unwrap();
    assert!(json.contains("forensics://cases"));
}

#[test]
fn test_mcp_tool() {
    let tool = McpTool {
        name: "search_files".to_string(),
        description: "Search files".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        }),
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("search_files"));
}

#[test]
fn test_mcp_config_default() {
    let config = McpConfig::default();
    assert!(config.servers.is_empty());
    assert!(config.resources.is_empty());
    assert!(config.tools.is_empty());
}

#[test]
fn test_mcp_capabilities_default() {
    let caps = McpCapabilities::default();
    assert!(!caps.resources);
    assert!(!caps.tools);
    assert!(!caps.prompts);
}

#[test]
fn test_json_rpc_request() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "initialize".to_string(),
        params: Some(serde_json::json!({})),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"initialize\""));
}

#[test]
fn test_mcp_server_status() {
    let status = McpServerStatus {
        id: "test".to_string(),
        name: "Test".to_string(),
        connected: true,
        capabilities: McpCapabilities {
            resources: true,
            tools: true,
            prompts: false,
        },
        last_error: None,
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"connected\":true"));
    assert!(json.contains("\"resources\":true"));
}

#[test]
fn validates_and_normalizes_sse_urls() {
    let mut url = " http://localhost:3001/sse ".to_string();
    validate_sse_url(&mut url).unwrap();
    assert_eq!(url, "http://localhost:3001/sse");
}

#[test]
fn rejects_unsupported_sse_url_schemes() {
    let mut url = "file:///tmp/mcp.sock".to_string();
    let err = validate_sse_url(&mut url).unwrap_err();
    assert!(err.to_string().contains("Unsupported MCP SSE URL scheme"));
}

#[test]
fn rejects_sse_urls_with_embedded_credentials() {
    let mut url = "https://user:pass@example.com/sse".to_string();
    let err = validate_sse_url(&mut url).unwrap_err();
    assert!(err.to_string().contains("embedded credentials"));
}

#[test]
fn validates_and_normalizes_stdio_command() {
    let mut command = " node ".to_string();
    validate_stdio_command(&mut command, &["server.js".to_string()]).unwrap();
    assert_eq!(command, "node");
}

#[test]
fn rejects_stdio_command_paths() {
    let mut command = "./node".to_string();
    let err = validate_stdio_command(&mut command, &[]).unwrap_err();
    assert!(err.to_string().contains("not a path"));
}

#[test]
fn rejects_duplicate_mcp_server_ids() {
    let mut config = McpConfig {
        servers: vec![
            McpServerConfig {
                id: "srv".to_string(),
                name: "One".to_string(),
                transport: McpTransport::Sse {
                    url: "http://localhost:3001/sse".to_string(),
                },
                enabled: true,
                auto_connect: false,
                permissions: localhost_permissions(),
            },
            McpServerConfig {
                id: " srv ".to_string(),
                name: "Two".to_string(),
                transport: McpTransport::Sse {
                    url: "http://localhost:3002/sse".to_string(),
                },
                enabled: true,
                auto_connect: false,
                permissions: localhost_permissions(),
            },
        ],
        resources: HashMap::new(),
        tools: HashMap::new(),
    };

    let err = validate_mcp_config(&mut config).unwrap_err();
    assert!(err.to_string().contains("Duplicate MCP server id"));
}

#[test]
fn validates_localhost_policy_for_sse() {
    let mut config = McpServerConfig {
        id: "srv".to_string(),
        name: "Local".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001/sse".to_string(),
        },
        enabled: true,
        auto_connect: false,
        permissions: localhost_permissions(),
    };
    validate_mcp_server_config(&mut config).unwrap();

    config.transport = McpTransport::Sse {
        url: "https://example.com/sse".to_string(),
    };
    let err = validate_mcp_server_config(&mut config).unwrap_err();
    assert!(err.to_string().contains("localhost only policy"));
}

#[test]
fn validates_stdio_permissions_against_allow_list() {
    let mut config = McpServerConfig {
        id: "srv".to_string(),
        name: "Proc".to_string(),
        transport: McpTransport::Stdio {
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile {
            allowed_commands: vec!["python".to_string()],
            ..McpPermissionProfile::default()
        },
    };
    let err = validate_mcp_server_config(&mut config).unwrap_err();
    assert!(err.to_string().contains("allowed command list"));
}

#[test]
fn fills_stdio_allow_list_with_command_by_default() {
    let mut config = McpServerConfig {
        id: "srv".to_string(),
        name: "Proc".to_string(),
        transport: McpTransport::Stdio {
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    };
    validate_mcp_server_config(&mut config).unwrap();
    assert_eq!(config.permissions.allowed_commands, vec!["node"]);
}

#[test]
fn normalizes_allowed_command_casing_and_spacing() {
    let mut config = McpServerConfig {
        id: "srv".to_string(),
        name: "Proc".to_string(),
        transport: McpTransport::Stdio {
            command: "Node".to_string(),
            args: vec!["server.js".to_string()],
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile {
            allowed_commands: vec![
                " node ".to_string(),
                "NODE".to_string(),
                "python".to_string(),
            ],
            ..McpPermissionProfile::default()
        },
    };
    validate_mcp_server_config(&mut config).unwrap();
    assert_eq!(config.permissions.allowed_commands, vec!["node", "python"]);
    match &config.transport {
        McpTransport::Stdio { command, .. } => assert_eq!(command, "Node"),
        _ => panic!("expected stdio transport"),
    }
}
