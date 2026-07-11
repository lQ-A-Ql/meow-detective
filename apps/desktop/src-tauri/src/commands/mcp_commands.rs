//! MCP (Model Context Protocol) command facade.

mod config;
mod lifecycle;
mod mapping;
mod prompts;
mod resources;
mod tools;

pub use config::{add_mcp_server, get_mcp_config, remove_mcp_server, save_mcp_config};
pub use lifecycle::{connect_mcp_server, disconnect_mcp_server, test_mcp_connection};
pub use prompts::{get_mcp_prompt, list_mcp_prompts};
pub use resources::list_mcp_resources;
pub use tools::{call_mcp_tool, list_mcp_tools};

#[cfg(test)]
#[path = "../../tests/unit/commands/mcp_commands_test.rs"]
mod tests;
