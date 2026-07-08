//! SSE (Server-Sent Events) Transport
//!
//! HTTP/SSE transport implementation for MCP servers.

use async_trait::async_trait;
use reqwest::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use super::McpTransportTrait;
use crate::error::{McpError, McpResult};
use crate::types::{validate_sse_url, *};

/// SSE Transport
///
/// Connects to MCP servers via HTTP with Server-Sent Events.
pub struct SseTransport {
    /// HTTP client (reusable for connection pooling)
    client: Client,
    /// Server URL
    url: String,
    /// Connection state
    connected: Arc<AtomicBool>,
    /// Request ID counter
    request_id: Arc<AtomicU64>,
    /// Server capabilities
    capabilities: Arc<Mutex<Option<McpCapabilities>>>,
}

impl SseTransport {
    /// Create a new SSE transport
    pub fn new(url: &str) -> McpResult<Self> {
        let mut url = url.to_string();
        validate_sse_url(&mut url)?;

        // Build a reusable HTTP client with connection pooling
        let client = Client::builder()
            .pool_max_idle_per_host(10)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Ok(Self {
            client,
            url,
            connected: Arc::new(AtomicBool::new(false)),
            request_id: Arc::new(AtomicU64::new(1)),
            capabilities: Arc::new(Mutex::new(None)),
        })
    }

    /// Send a JSON-RPC request
    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        debug!("Sending JSON-RPC request: {:?}", request);

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpError::Connection(format!("Failed to send request: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            return Err(McpError::Connection(format!("HTTP error: {}", status)));
        }

        let rpc_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| McpError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        debug!("Received JSON-RPC response: {:?}", rpc_response);

        if let Some(error) = rpc_response.error {
            return Err(McpError::Server {
                code: error.code,
                message: error.message,
            });
        }

        rpc_response
            .result
            .ok_or_else(|| McpError::InvalidResponse("No result in response".to_string()))
    }
}

#[async_trait]
impl McpTransportTrait for SseTransport {
    async fn initialize(&mut self) -> McpResult<McpCapabilities> {
        info!("Initializing MCP connection to {}", self.url);

        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {
                roots: Some(RootsCapability {
                    list_changed: false,
                }),
            },
            client_info: ClientInfo {
                name: "Meow_Detective".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        let result = self
            .send_request(
                "initialize",
                Some(serde_json::to_value(params).map_err(McpError::Json)?),
            )
            .await?;

        // Parse capabilities from response
        let capabilities = McpCapabilities {
            resources: result
                .get("capabilities")
                .and_then(|c| c.get("resources"))
                .is_some(),
            tools: result
                .get("capabilities")
                .and_then(|c| c.get("tools"))
                .is_some(),
            prompts: result
                .get("capabilities")
                .and_then(|c| c.get("prompts"))
                .is_some(),
        };

        *self.capabilities.lock().await = Some(capabilities.clone());
        self.connected.store(true, Ordering::SeqCst);

        info!("MCP connection initialized successfully");
        Ok(capabilities)
    }

    async fn list_resources(&self) -> McpResult<Vec<McpResource>> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let result = self.send_request("resources/list", None).await?;

        let resources: Vec<McpResource> = serde_json::from_value(
            result
                .get("resources")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| McpError::InvalidResponse(format!("Failed to parse resources: {}", e)))?;

        Ok(resources)
    }

    async fn read_resource(&self, uri: &str) -> McpResult<String> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let params = ResourceReadParams {
            uri: uri.to_string(),
        };

        let result = self
            .send_request(
                "resources/read",
                Some(serde_json::to_value(params).map_err(McpError::Json)?),
            )
            .await?;

        // Extract content from response
        let content = result
            .get("contents")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .ok_or_else(|| McpError::InvalidResponse("No content in resource".to_string()))?;

        Ok(content.to_string())
    }

    async fn list_tools(&self) -> McpResult<Vec<McpTool>> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let result = self.send_request("tools/list", None).await?;

        let tools: Vec<McpTool> = serde_json::from_value(
            result
                .get("tools")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| McpError::InvalidResponse(format!("Failed to parse tools: {}", e)))?;

        Ok(tools)
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<serde_json::Value> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let params = ToolCallParams {
            name: name.to_string(),
            arguments,
        };

        let result = self
            .send_request(
                "tools/call",
                Some(serde_json::to_value(params).map_err(McpError::Json)?),
            )
            .await?;

        Ok(result)
    }

    async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let result = self.send_request("prompts/list", None).await?;

        let prompts: Vec<McpPrompt> = serde_json::from_value(
            result
                .get("prompts")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| McpError::InvalidResponse(format!("Failed to parse prompts: {}", e)))?;

        Ok(prompts)
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<std::collections::HashMap<String, String>>,
    ) -> McpResult<String> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(McpError::NotConnected);
        }

        let params = PromptGetParams {
            name: name.to_string(),
            arguments,
        };

        let result = self
            .send_request(
                "prompts/get",
                Some(serde_json::to_value(params).map_err(McpError::Json)?),
            )
            .await?;

        // Extract prompt content
        let content = result
            .get("messages")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.first())
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| McpError::InvalidResponse("No content in prompt".to_string()))?;

        Ok(content.to_string())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn disconnect(&mut self) -> McpResult<()> {
        info!("Disconnecting from MCP server {}", self.url);
        self.connected.store(false, Ordering::SeqCst);
        *self.capabilities.lock().await = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_transport_new() {
        let transport = SseTransport::new("http://localhost:3001").unwrap();
        assert_eq!(transport.url, "http://localhost:3001/");
        assert!(!transport.is_connected());
    }

    #[test]
    fn test_request_id_increment() {
        let transport = SseTransport::new("http://localhost:3001").unwrap();
        let id1 = transport.request_id.fetch_add(1, Ordering::SeqCst);
        let id2 = transport.request_id.fetch_add(1, Ordering::SeqCst);
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_sse_transport_rejects_invalid_url() {
        let Err(err) = SseTransport::new("file:///tmp/mcp.sock") else {
            panic!("expected invalid SSE URL to fail");
        };
        assert!(err.to_string().contains("Unsupported MCP SSE URL scheme"));
    }
}
