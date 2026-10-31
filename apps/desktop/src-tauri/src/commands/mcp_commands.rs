//! MCP (Model Context Protocol) command facade.

mod host;

pub use host::{
    call_mcp_host_tool, get_mcp_host_status, list_mcp_host_tools, set_mcp_host_settings,
};
