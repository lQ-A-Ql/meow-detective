//! MCP Client Integration Tests

use mcp_client::*;

#[test]
fn test_mcp_server_config_creation() {
    let config = McpServerConfig {
        id: "test-server".to_string(),
        name: "Test MCP Server".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: false,
    };

    assert_eq!(config.id, "test-server");
    assert_eq!(config.name, "Test MCP Server");
    assert!(config.enabled);
    assert!(!config.auto_connect);
}

#[test]
fn test_mcp_config_serialization_roundtrip() {
    let config = McpConfig {
        servers: vec![
            McpServerConfig {
                id: "server1".to_string(),
                name: "Server 1".to_string(),
                transport: McpTransport::Sse {
                    url: "http://localhost:3001".to_string(),
                },
                enabled: true,
                auto_connect: true,
            },
            McpServerConfig {
                id: "server2".to_string(),
                name: "Server 2".to_string(),
                transport: McpTransport::Stdio {
                    command: "python".to_string(),
                    args: vec!["-m".to_string(), "server".to_string()],
                },
                enabled: false,
                auto_connect: false,
            },
        ],
        resources: vec![
            ("cases".to_string(), true),
            ("files".to_string(), true),
            ("timeline".to_string(), false),
        ].into_iter().collect(),
        tools: vec![
            ("search_files".to_string(), true),
            ("get_file_content".to_string(), true),
        ].into_iter().collect(),
    };

    // Serialize
    let json = serde_json::to_string_pretty(&config).unwrap();
    assert!(json.contains("server1"));
    assert!(json.contains("server2"));
    assert!(json.contains("cases"));

    // Deserialize
    let deserialized: McpConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.servers.len(), 2);
    assert_eq!(deserialized.servers[0].id, "server1");
    assert_eq!(deserialized.servers[1].id, "server2");
}

#[test]
fn test_mcp_server_status() {
    let status = McpServerStatus {
        id: "test".to_string(),
        name: "Test Server".to_string(),
        connected: true,
        capabilities: McpCapabilities {
            resources: true,
            tools: true,
            prompts: true,
        },
        last_error: None,
    };

    assert!(status.connected);
    assert!(status.capabilities.resources);
    assert!(status.capabilities.tools);
    assert!(status.capabilities.prompts);
    assert!(status.last_error.is_none());
}

#[test]
fn test_mcp_resource_list() {
    let resources = vec![
        McpResource {
            uri: "forensics://cases".to_string(),
            name: "Cases".to_string(),
            description: Some("List of cases".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        McpResource {
            uri: "forensics://files".to_string(),
            name: "Files".to_string(),
            description: None,
            mime_type: None,
        },
    ];

    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].uri, "forensics://cases");
    assert_eq!(resources[1].uri, "forensics://files");
}

#[test]
fn test_mcp_tool_list() {
    let tools = vec![
        McpTool {
            name: "search_files".to_string(),
            description: "Search files".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "get_file_content".to_string(),
            description: "Get file content".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_id": { "type": "string" },
                    "format": { "type": "string", "enum": ["text", "hex"] }
                },
                "required": ["file_id"]
            }),
        },
    ];

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "search_files");
    assert_eq!(tools[1].name, "get_file_content");
}

#[test]
fn test_mcp_prompt_with_arguments() {
    let prompt = McpPrompt {
        name: "analyze_timeline".to_string(),
        description: Some("Analyze timeline".to_string()),
        arguments: vec![
            McpPromptArgument {
                name: "time_start".to_string(),
                description: Some("Start time".to_string()),
                required: true,
            },
            McpPromptArgument {
                name: "time_end".to_string(),
                description: Some("End time".to_string()),
                required: false,
            },
        ],
    };

    assert_eq!(prompt.arguments.len(), 2);
    assert!(prompt.arguments[0].required);
    assert!(!prompt.arguments[1].required);
}

#[test]
fn test_mcp_client_lifecycle() {
    let config = McpServerConfig {
        id: "test".to_string(),
        name: "Test".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: false,
    };

    let client = McpClient::new(config);

    // Initial state
    assert!(!client.is_connected());
    assert_eq!(client.config().id, "test");
}

#[tokio::test]
async fn test_mcp_client_operations_when_not_connected() {
    let config = McpServerConfig {
        id: "test".to_string(),
        name: "Test".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: false,
    };

    let client = McpClient::new(config);

    // All operations should return NotConnected error
    assert!(matches!(client.list_resources().await, Err(McpError::NotConnected)));
    assert!(matches!(client.list_tools().await, Err(McpError::NotConnected)));
    assert!(matches!(client.list_prompts().await, Err(McpError::NotConnected)));
    assert!(matches!(client.read_resource("test").await, Err(McpError::NotConnected)));
    assert!(matches!(client.call_tool("test", serde_json::json!({})).await, Err(McpError::NotConnected)));
    assert!(matches!(client.get_prompt("test", None).await, Err(McpError::NotConnected)));
}

#[test]
fn test_mcp_error_types() {
    let errors = vec![
        McpError::Connection("test".to_string()),
        McpError::Transport("test".to_string()),
        McpError::Protocol("test".to_string()),
        McpError::Timeout,
        McpError::NotConnected,
        McpError::InvalidResponse("test".to_string()),
        McpError::ToolNotFound("test".to_string()),
        McpError::ResourceNotFound("test".to_string()),
        McpError::PromptNotFound("test".to_string()),
        McpError::Server { code: -1, message: "test".to_string() },
    ];

    // All errors should have a display implementation
    for error in errors {
        let _ = error.to_string();
    }
}

#[test]
fn test_json_rpc_request_format() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "forensics-workbench",
                "version": "0.1.0"
            }
        })),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"initialize\""));
    assert!(json.contains("forensics-workbench"));
}

#[test]
fn test_json_rpc_response_parsing() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "capabilities": {
                "resources": true,
                "tools": true
            }
        }
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, Some(1));
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[test]
fn test_json_rpc_error_response_parsing() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": -32600,
            "message": "Invalid Request"
        }
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(response.result.is_none());
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    assert_eq!(error.code, -32600);
    assert_eq!(error.message, "Invalid Request");
}
