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
        if matches!(
            self.config.permissions.resource_access,
            McpResourceAccess::Disabled
        ) {
            return Err(McpError::Protocol(
                "MCP resource access is disabled for this server".to_string(),
            ));
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_resources().await
    }

    /// Read a resource
    pub async fn read_resource(&self, uri: &str) -> McpResult<String> {
        if matches!(
            self.config.permissions.resource_access,
            McpResourceAccess::Disabled
        ) {
            return Err(McpError::Protocol(
                "MCP resource access is disabled for this server".to_string(),
            ));
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.read_resource(uri).await
    }

    /// List available tools
    pub async fn list_tools(&self) -> McpResult<Vec<McpTool>> {
        if matches!(self.config.permissions.tool_access, McpToolAccess::Disabled) {
            return Err(McpError::Protocol(
                "MCP tool access is disabled for this server".to_string(),
            ));
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_tools().await
    }

    /// Call a tool
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<serde_json::Value> {
        match self.config.permissions.tool_access {
            McpToolAccess::Disabled => {
                return Err(McpError::Protocol(
                    "MCP tool access is disabled for this server".to_string(),
                ))
            }
            McpToolAccess::AllowList => {
                if !self
                    .config
                    .permissions
                    .allowed_tools
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(name))
                {
                    return Err(McpError::ToolNotFound(format!(
                        "{} (not permitted by allow list)",
                        name
                    )));
                }
            }
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.call_tool(name, arguments).await
    }

    /// List available prompts
    pub async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>> {
        if matches!(
            self.config.permissions.prompt_access,
            McpPromptAccess::Disabled
        ) {
            return Err(McpError::Protocol(
                "MCP prompt access is disabled for this server".to_string(),
            ));
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.list_prompts().await
    }

    /// Get a prompt
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> McpResult<String> {
        if matches!(
            self.config.permissions.prompt_access,
            McpPromptAccess::Disabled
        ) {
            return Err(McpError::Protocol(
                "MCP prompt access is disabled for this server".to_string(),
            ));
        }
        let transport = self.transport.as_ref().ok_or(McpError::NotConnected)?;
        transport.get_prompt(name, arguments).await
    }
}

#[cfg(test)]
#[path = "../tests/unit/client.rs"]
mod tests;
