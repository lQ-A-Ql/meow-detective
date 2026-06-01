//! MCP Transport Layer
//!
//! Transport implementations for connecting to MCP servers.

pub mod sse;

use crate::error::McpResult;
use crate::types::*;
use async_trait::async_trait;

/// MCP Transport trait
///
/// All transport implementations must implement this trait.
#[async_trait]
pub trait McpTransportTrait: Send + Sync {
    /// Initialize the connection
    async fn initialize(&mut self) -> McpResult<McpCapabilities>;

    /// List available resources
    async fn list_resources(&self) -> McpResult<Vec<McpResource>>;

    /// Read a resource
    async fn read_resource(&self, uri: &str) -> McpResult<String>;

    /// List available tools
    async fn list_tools(&self) -> McpResult<Vec<McpTool>>;

    /// Call a tool
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<serde_json::Value>;

    /// List available prompts
    async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>>;

    /// Get a prompt
    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<std::collections::HashMap<String, String>>,
    ) -> McpResult<String>;

    /// Check if connected
    fn is_connected(&self) -> bool;

    /// Disconnect
    async fn disconnect(&mut self) -> McpResult<()>;
}
