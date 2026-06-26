//! MCP (Model Context Protocol) Client Library
//!
//! This crate provides a Rust client for connecting to MCP servers.
//! It supports both SSE (HTTP) and Stdio transport methods.

pub mod client;
pub mod error;
pub mod probe;
pub mod transport;
pub mod types;

pub use client::McpClient;
pub use error::McpError;
pub use error::McpResult;
pub use types::*;
