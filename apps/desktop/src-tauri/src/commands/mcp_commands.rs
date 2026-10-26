//! MCP (Model Context Protocol) command facade.

mod access;
mod host;

pub use access::{create_mcp_host_client, list_mcp_host_clients, revoke_mcp_host_client};

pub use host::{
    call_mcp_host_tool, get_mcp_host_status, list_mcp_host_tools, set_mcp_host_settings,
};
