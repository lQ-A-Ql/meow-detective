//! MCP protocol types and validation helpers.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::Path;

use crate::error::{McpError, McpResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub enabled: bool,
    pub auto_connect: bool,
    #[serde(default)]
    pub permissions: McpPermissionProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpTransport {
    Sse { url: String },
    Stdio { command: String, args: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum McpResourceAccess {
    #[default]
    ReadOnly,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum McpToolAccess {
    #[default]
    Disabled,
    AllowList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum McpPromptAccess {
    #[default]
    ReadOnly,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum McpNetworkPolicy {
    #[default]
    LocalhostOnly,
    PrivateLanAllowed,
    AnyHost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPermissionProfile {
    #[serde(default)]
    pub resource_access: McpResourceAccess,
    #[serde(default)]
    pub tool_access: McpToolAccess,
    #[serde(default)]
    pub prompt_access: McpPromptAccess,
    #[serde(default)]
    pub network_policy: McpNetworkPolicy,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub allowed_commands: Vec<String>,
}

impl Default for McpPermissionProfile {
    fn default() -> Self {
        Self {
            resource_access: McpResourceAccess::ReadOnly,
            tool_access: McpToolAccess::Disabled,
            prompt_access: McpPromptAccess::ReadOnly,
            network_policy: McpNetworkPolicy::LocalhostOnly,
            allowed_tools: Vec::new(),
            allowed_commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub capabilities: McpCapabilities,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub resources: bool,
    pub tools: bool,
    pub prompts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
    pub resources: HashMap<String, bool>,
    pub tools: HashMap<String, bool>,
}

pub fn validate_mcp_config(config: &mut McpConfig) -> McpResult<()> {
    let mut seen_server_ids = HashSet::new();

    for server in &mut config.servers {
        server.id = server.id.trim().to_string();
        server.name = server.name.trim().to_string();

        if server.id.is_empty() {
            return Err(McpError::Protocol("MCP server id is required".to_string()));
        }
        if !seen_server_ids.insert(server.id.clone()) {
            return Err(McpError::Protocol(format!(
                "Duplicate MCP server id: {}",
                server.id
            )));
        }
        if server.name.is_empty() {
            return Err(McpError::Protocol(format!(
                "MCP server {} name is required",
                server.id
            )));
        }

        validate_mcp_transport(&mut server.transport)?;
        validate_mcp_permissions(server)?;
    }

    Ok(())
}

pub fn validate_mcp_server_config(config: &mut McpServerConfig) -> McpResult<()> {
    let mut mcp_config = McpConfig {
        servers: vec![config.clone()],
        resources: HashMap::new(),
        tools: HashMap::new(),
    };

    validate_mcp_config(&mut mcp_config)?;
    *config = mcp_config
        .servers
        .into_iter()
        .next()
        .ok_or_else(|| McpError::Protocol("MCP server config was not preserved".to_string()))?;
    Ok(())
}

pub fn validate_mcp_transport(transport: &mut McpTransport) -> McpResult<()> {
    match transport {
        McpTransport::Sse { url } => validate_sse_url(url),
        McpTransport::Stdio { command, args } => validate_stdio_command(command, args),
    }
}

pub fn validate_mcp_permissions(config: &mut McpServerConfig) -> McpResult<()> {
    normalize_string_list(&mut config.permissions.allowed_tools);
    normalize_string_list(&mut config.permissions.allowed_commands);

    match &mut config.transport {
        McpTransport::Sse { url } => {
            validate_sse_url_with_policy(url, &config.permissions.network_policy)?;
        }
        McpTransport::Stdio { command, args } => {
            validate_stdio_command(command, args)?;
            if config.permissions.allowed_commands.is_empty() {
                config.permissions.allowed_commands.push(command.clone());
            }
            if !config
                .permissions
                .allowed_commands
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(command))
            {
                return Err(McpError::Protocol(
                    "MCP stdio command is not present in the allowed command list".to_string(),
                ));
            }
        }
    }

    Ok(())
}

pub fn validate_sse_url(url: &mut String) -> McpResult<()> {
    validate_sse_url_with_policy(url, &McpNetworkPolicy::AnyHost)
}

pub fn validate_sse_url_with_policy(url: &mut String, policy: &McpNetworkPolicy) -> McpResult<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(McpError::Protocol("MCP SSE URL is required".to_string()));
    }

    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|e| McpError::Protocol(format!("Invalid MCP SSE URL: {}", e)))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(McpError::Protocol(format!(
                "Unsupported MCP SSE URL scheme: {}",
                scheme
            )))
        }
    }
    if parsed.host_str().is_none() {
        return Err(McpError::Protocol(
            "MCP SSE URL must include a host".to_string(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(McpError::Protocol(
            "MCP SSE URL must not include embedded credentials".to_string(),
        ));
    }
    if !sse_host_allowed(&parsed, policy) {
        let detail = match policy {
            McpNetworkPolicy::LocalhostOnly => "localhost only",
            McpNetworkPolicy::PrivateLanAllowed => "private-lan only",
            McpNetworkPolicy::AnyHost => "configured",
        };
        return Err(McpError::Protocol(format!(
            "MCP SSE URL host is not allowed by the {} policy",
            detail
        )));
    }

    *url = parsed.to_string();
    Ok(())
}

pub fn validate_stdio_command(command: &mut String, args: &[String]) -> McpResult<()> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(McpError::Protocol(
            "MCP stdio command is required".to_string(),
        ));
    }
    if trimmed.contains('\0') || args.iter().any(|arg| arg.contains('\0')) {
        return Err(McpError::Protocol(
            "MCP stdio command and args must not contain NUL bytes".to_string(),
        ));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() || path.components().count() > 1 {
        return Err(McpError::Protocol(
            "MCP stdio command must be an executable name, not a path".to_string(),
        ));
    }

    *command = trimmed.to_string();
    Ok(())
}

fn normalize_string_list(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    *values = normalized;
}

fn sse_host_allowed(url: &reqwest::Url, policy: &McpNetworkPolicy) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    match policy {
        McpNetworkPolicy::AnyHost => true,
        McpNetworkPolicy::LocalhostOnly => is_localhost(host),
        McpNetworkPolicy::PrivateLanAllowed => is_localhost(host) || is_private_lan_host(host),
    }
}

fn is_localhost(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn is_private_lan_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
            }
            IpAddr::V6(v6) => v6.is_unique_local(),
        };
    }

    !host.contains('.') || host.ends_with(".local") || host.ends_with(".lan")
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

#[derive(Debug, Serialize, Default)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
}

#[derive(Debug, Serialize)]
pub struct RootsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct PromptGetParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, String>>,
}

#[cfg(test)]
#[path = "../tests/unit/types.rs"]
mod tests;
