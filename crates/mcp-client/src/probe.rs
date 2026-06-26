//! MCP connection probing.
//!
//! Provides a one-shot probe that validates transport parameters,
//! connects to an MCP server, discovers capabilities, and disconnects.

use crate::client::McpClient;
use crate::error::McpResult;
use crate::types::{
    validate_mcp_server_config, McpCapabilities, McpPermissionProfile, McpServerConfig,
    McpTransport,
};

/// Probe an MCP server by connecting, discovering capabilities, and disconnecting.
///
/// `transport_type` must be `"sse"` or `"stdio"`. All other values return a
/// `McpError::Protocol` with a descriptive message.
pub async fn probe_mcp_connection(
    transport_type: &str,
    url: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    permissions: McpPermissionProfile,
) -> McpResult<McpCapabilities> {
    let transport = match transport_type {
        "sse" => McpTransport::Sse {
            url: url.unwrap_or_default(),
        },
        "stdio" => McpTransport::Stdio {
            command: command.unwrap_or_default(),
            args: args.unwrap_or_default(),
        },
        other => {
            return Err(crate::error::McpError::Protocol(format!(
                "Invalid transport type: {other}"
            )));
        }
    };

    let mut config = McpServerConfig {
        id: "test".to_string(),
        name: "Test".to_string(),
        transport,
        enabled: true,
        auto_connect: false,
        permissions,
    };

    validate_mcp_server_config(&mut config)?;

    let mut client = McpClient::new(config);
    let capabilities = client.connect().await?;
    let _ = client.disconnect().await;
    Ok(capabilities)
}
