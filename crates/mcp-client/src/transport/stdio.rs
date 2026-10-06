//! Stdio Transport
//!
//! Local process transport implementation for MCP servers.
//! Communicates with a child process via stdin/stdout using JSON-RPC.

use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use tracing::{debug, info};

use super::McpTransportTrait;
use crate::error::{McpError, McpResult};
use crate::types::{validate_stdio_command, *};

/// Stdio Transport
///
/// Connects to MCP servers via a local child process's stdin/stdout.
pub struct StdioTransport {
    /// Child process handle
    child: Arc<Mutex<Option<Child>>>,
    /// Writer to child stdin
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    /// Reader from child stdout
    stdout: Arc<Mutex<Option<BufReader<tokio::process::ChildStdout>>>>,
    /// Connection state
    connected: Arc<AtomicBool>,
    /// Request ID counter
    request_id: Arc<AtomicU64>,
    /// Serializes request write/read pairs so each response remains associated
    /// with the request that produced it.
    request_lock: Arc<Mutex<()>>,
    /// Command to spawn
    command: String,
    /// Arguments for the command
    args: Vec<String>,
    /// Server capabilities
    capabilities: Arc<Mutex<Option<McpCapabilities>>>,
}

impl StdioTransport {
    /// Create a new Stdio transport
    pub fn new(command: &str, args: &[String]) -> McpResult<Self> {
        let mut command = command.to_string();
        validate_stdio_command(&mut command, args)?;

        Ok(Self {
            child: Arc::new(Mutex::new(None)),
            stdin: Arc::new(Mutex::new(None)),
            stdout: Arc::new(Mutex::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
            request_id: Arc::new(AtomicU64::new(1)),
            request_lock: Arc::new(Mutex::new(())),
            command,
            args: args.to_vec(),
            capabilities: Arc::new(Mutex::new(None)),
        })
    }

    /// Send a JSON-RPC request and read the response
    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let _request_guard = self.request_lock.lock().await;
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let mut request_json = serde_json::to_string(&request).map_err(McpError::Json)?;
        request_json.push('\n');

        debug!(method, id, "Sending JSON-RPC request via stdio");

        // Write request to stdin
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard.as_mut().ok_or(McpError::NotConnected)?;
            stdin
                .write_all(request_json.as_bytes())
                .await
                .map_err(|e| McpError::Transport(format!("Failed to write to stdin: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| McpError::Transport(format!("Failed to flush stdin: {}", e)))?;
        }

        // Read response from stdout
        let response_line = {
            let mut stdout_guard = self.stdout.lock().await;
            let stdout = stdout_guard.as_mut().ok_or(McpError::NotConnected)?;
            let mut line = String::new();
            timeout(Duration::from_secs(30), stdout.read_line(&mut line))
                .await
                .map_err(|_| McpError::Timeout)?
                .map_err(|e| McpError::Transport(format!("Failed to read from stdout: {}", e)))?;
            if line.is_empty() {
                return Err(McpError::Transport("Process stdout closed".to_string()));
            }
            line
        };

        debug!(method, id, "Received JSON-RPC response via stdio");

        let rpc_response: JsonRpcResponse = serde_json::from_str(&response_line)
            .map_err(|e| McpError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        if rpc_response.id != Some(id) {
            return Err(McpError::InvalidResponse(format!(
                "Response id does not match request id {}",
                id
            )));
        }

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

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<()> {
        let _request_guard = self.request_lock.lock().await;
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        let mut notification_json = serde_json::to_string(&notification).map_err(McpError::Json)?;
        notification_json.push('\n');
        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard.as_mut().ok_or(McpError::NotConnected)?;
        stdin
            .write_all(notification_json.as_bytes())
            .await
            .map_err(|e| McpError::Transport(format!("Failed to write notification: {}", e)))?;
        stdin
            .flush()
            .await
            .map_err(|e| McpError::Transport(format!("Failed to flush notification: {}", e)))?;
        debug!(method, "Sent JSON-RPC notification via stdio");
        Ok(())
    }
}

#[async_trait]
impl McpTransportTrait for StdioTransport {
    async fn initialize(&mut self) -> McpResult<McpCapabilities> {
        info!(
            command = %self.command,
            arg_count = self.args.len(),
            "Starting MCP server process"
        );

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| McpError::Connection(format!("Failed to spawn process: {}", e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Connection("Failed to open stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Connection("Failed to open stdout".to_string()))?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut stderr = BufReader::new(stderr);
                let mut line = String::new();
                let mut bytes = 0usize;
                while stderr.read_line(&mut line).await.unwrap_or(0) > 0 {
                    bytes = bytes.saturating_add(line.len());
                    line.clear();
                }
                debug!(target: "mcp_client", stderr_bytes = bytes, "MCP server stderr stream closed");
            });
        }

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(BufReader::new(stdout));
        self.connected.store(true, Ordering::SeqCst);

        // Send initialize request
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

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        info!("MCP stdio connection initialized successfully");
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
        arguments: Option<HashMap<String, String>>,
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
        info!("Disconnecting MCP stdio transport");
        self.connected.store(false, Ordering::SeqCst);
        *self.capabilities.lock().await = None;
        *self.stdin.lock().await = None;
        *self.stdout.lock().await = None;

        // Kill the child process
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/transport/stdio.rs"]
mod tests;
