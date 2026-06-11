//! MCP Client
//!
//! High-level client for connecting to MCP servers.

use std::collections::HashMap;
use tracing::info;

use crate::error::{McpError, McpResult};
use crate::transport::sse::SseTransport;
use crate::transport::stdio::StdioTransport;
use crate::transport::McpTransportTrait;
use crate::types::*;

/// MCP Client
///
/// High-level client for connecting to MCP servers.
pub struct McpClient {
    /// Server configuration
    config: McpServerConfig,
    /// Transport implementation
    transport: Option<Box<dyn McpTransportTrait>>,
    /// Connection state
    connected: bool,
    /// Capabilities returned by the server during initialization.
    capabilities: Option<McpCapabilities>,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            transport: None,
            connected: false,
            capabilities: None,
        }
    }

    /// Connect to the MCP server
    pub async fn connect(&mut self) -> McpResult<McpCapabilities> {
        info!("Connecting to MCP server: {}", self.config.name);

        let mut transport: Box<dyn McpTransportTrait> = match &self.config.transport {
            McpTransport::Sse { url } => Box::new(SseTransport::new(url)?),
            McpTransport::Stdio { command, args } => Box::new(StdioTransport::new(command, args)?),
        };

        let capabilities = transport.initialize().await?;
        self.transport = Some(transport);
        self.connected = true;
        self.capabilities = Some(capabilities.clone());

        info!("Connected to MCP server: {}", self.config.name);
        Ok(capabilities)
    }

    /// Disconnect from the MCP server
    pub async fn disconnect(&mut self) -> McpResult<()> {
        if let Some(transport) = &mut self.transport {
            transport.disconnect().await?;
        }
        self.transport = None;
        self.connected = false;
        self.capabilities = None;
        info!("Disconnected from MCP server: {}", self.config.name);
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get server configuration
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Get capabilities returned by the server during initialization.
    pub fn capabilities(&self) -> Option<&McpCapabilities> {
        self.capabilities.as_ref()
    }

    /// List available resources
    pub async fn list_resources(&self) -> McpResult<Vec<McpResource>> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_resources().await
    }

    /// Read a resource
    pub async fn read_resource(&self, uri: &str) -> McpResult<String> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.read_resource(uri).await
    }

    /// List available tools
    pub async fn list_tools(&self) -> McpResult<Vec<McpTool>> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_tools().await
    }

    /// Call a tool
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<serde_json::Value> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.call_tool(name, arguments).await
    }

    /// List available prompts
    pub async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_prompts().await
    }

    /// Get a prompt
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> McpResult<String> {
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.get_prompt(name, arguments).await
    }
}

#[cfg(test)]
mod tests {
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
        assert!(matches!(result, Err(McpError::NotConnected)));
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
        assert!(matches!(result, Err(McpError::NotConnected)));
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
        };
        let mut client = McpClient::new(config);
        assert!(matches!(client.connect().await, Err(McpError::Protocol(_))));
        assert!(!client.is_connected());
    }
}
