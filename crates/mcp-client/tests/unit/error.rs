use super::*;

#[test]
fn test_error_display_connection() {
    let err = McpError::Connection("timeout".to_string());
    assert_eq!(err.to_string(), "Connection error: timeout");
}

#[test]
fn test_error_display_transport() {
    let err = McpError::Transport("SSE disconnected".to_string());
    assert_eq!(err.to_string(), "Transport error: SSE disconnected");
}

#[test]
fn test_error_display_protocol() {
    let err = McpError::Protocol("invalid message".to_string());
    assert_eq!(err.to_string(), "Protocol error: invalid message");
}

#[test]
fn test_error_display_timeout() {
    let err = McpError::Timeout;
    assert_eq!(err.to_string(), "Connection timeout");
}

#[test]
fn test_error_display_not_connected() {
    let err = McpError::NotConnected;
    assert_eq!(err.to_string(), "Not connected to server");
}

#[test]
fn test_error_display_tool_not_found() {
    let err = McpError::ToolNotFound("search".to_string());
    assert_eq!(err.to_string(), "Tool not found: search");
}

#[test]
fn test_error_display_resource_not_found() {
    let err = McpError::ResourceNotFound("forensics://test".to_string());
    assert_eq!(err.to_string(), "Resource not found: forensics://test");
}

#[test]
fn test_error_display_server() {
    let err = McpError::Server {
        code: -32600,
        message: "Invalid Request".to_string(),
    };
    assert_eq!(err.to_string(), "Server error: -32600 - Invalid Request");
}

#[test]
fn test_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: McpError = io_err.into();
    assert!(err.to_string().contains("IO error"));
}

#[test]
fn test_error_from_json() {
    let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
    let err: McpError = json_err.into();
    assert!(err.to_string().contains("JSON error"));
}
