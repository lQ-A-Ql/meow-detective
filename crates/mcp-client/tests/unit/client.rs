use super::*;

fn create_test_config() -> McpServerConfig {
    McpServerConfig {
        id: "test".to_string(),
        name: "Test Server".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    }
}

#[test]
fn test_client_new() {
    let config = create_test_config();
    let client = McpClient::new(config);
    assert!(!client.is_connected());
    assert_eq!(client.config().name, "Test Server");
}

#[test]
fn test_client_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_client_not_connected_error() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.list_resources().await;
    assert!(matches!(result, Err(McpError::NotConnected)));
}

#[tokio::test]
async fn test_client_list_tools_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.list_tools().await;
    assert!(matches!(
        result,
        Err(McpError::NotConnected) | Err(McpError::Protocol(_))
    ));
}

#[tokio::test]
async fn test_client_list_prompts_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.list_prompts().await;
    assert!(matches!(result, Err(McpError::NotConnected)));
}

#[tokio::test]
async fn test_client_call_tool_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.call_tool("test", serde_json::json!({})).await;
    assert!(matches!(
        result,
        Err(McpError::NotConnected) | Err(McpError::Protocol(_))
    ));
}

#[tokio::test]
async fn test_client_read_resource_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.read_resource("test://resource").await;
    assert!(matches!(result, Err(McpError::NotConnected)));
}

#[tokio::test]
async fn test_client_get_prompt_not_connected() {
    let config = create_test_config();
    let client = McpClient::new(config);

    let result = client.get_prompt("test", None).await;
    assert!(matches!(result, Err(McpError::NotConnected)));
}

#[test]
fn test_client_config_sse() {
    let config = McpServerConfig {
        id: "sse-test".to_string(),
        name: "SSE Server".to_string(),
        transport: McpTransport::Sse {
            url: "http://localhost:3001".to_string(),
        },
        enabled: true,
        auto_connect: true,
        permissions: McpPermissionProfile::default(),
    };
    let client = McpClient::new(config);
    assert_eq!(client.config().id, "sse-test");
    assert!(client.config().auto_connect);
}

#[test]
fn test_client_config_stdio() {
    let config = McpServerConfig {
        id: "stdio-test".to_string(),
        name: "Stdio Server".to_string(),
        transport: McpTransport::Stdio {
            command: "python".to_string(),
            args: vec!["-m".to_string(), "server".to_string()],
        },
        enabled: false,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    };
    let client = McpClient::new(config);
    assert_eq!(client.config().id, "stdio-test");
    assert!(!client.config().enabled);
}

#[test]
fn test_client_capabilities_default_before_connect() {
    let config = create_test_config();
    let client = McpClient::new(config);
    assert!(client.capabilities().is_none());
}

#[tokio::test]
async fn test_client_rejects_invalid_transport_before_connect() {
    let config = McpServerConfig {
        id: "bad".to_string(),
        name: "Bad".to_string(),
        transport: McpTransport::Sse {
            url: "file:///tmp/server".to_string(),
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    };
    let mut client = McpClient::new(config);
    assert!(matches!(client.connect().await, Err(McpError::Protocol(_))));
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_client_blocks_disabled_tool_access_before_transport() {
    let config = McpServerConfig {
        id: "stdio-test".to_string(),
        name: "Stdio Server".to_string(),
        transport: McpTransport::Stdio {
            command: "python".to_string(),
            args: vec!["-m".to_string(), "server".to_string()],
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    };
    let client = McpClient::new(config);
    let result = client.call_tool("lookup", serde_json::json!({})).await;
    assert!(matches!(result, Err(McpError::Protocol(_))));
}

#[tokio::test]
async fn test_client_blocks_tool_not_in_allow_list() {
    let config = McpServerConfig {
        id: "stdio-test".to_string(),
        name: "Stdio Server".to_string(),
        transport: McpTransport::Stdio {
            command: "python".to_string(),
            args: vec!["-m".to_string(), "server".to_string()],
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile {
            tool_access: McpToolAccess::AllowList,
            allowed_tools: vec!["lookup".to_string()],
            allowed_commands: vec!["python".to_string()],
            ..McpPermissionProfile::default()
        },
    };
    let client = McpClient::new(config);
    let result = client.call_tool("download", serde_json::json!({})).await;
    assert!(matches!(result, Err(McpError::ToolNotFound(_))));
}
