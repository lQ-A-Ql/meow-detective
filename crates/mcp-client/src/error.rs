//! MCP Error Types
//!
//! Error types for the MCP client.

use thiserror::Error;

/// MCP 客户端错误
#[derive(Debug, Error)]
pub enum McpError {
    /// 连接错误
    #[error("Connection error: {0}")]
    Connection(String),

    /// 传输错误
    #[error("Transport error: {0}")]
    Transport(String),

    /// 协议错误
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// 连接超时
    #[error("Connection timeout")]
    Timeout,

    /// 未连接
    #[error("Not connected to server")]
    NotConnected,

    /// 无效响应
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// 工具未找到
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// 资源未找到
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// Prompt 未找到
    #[error("Prompt not found: {0}")]
    PromptNotFound(String),

    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 错误
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP 错误
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// 服务器错误
    #[error("Server error: {code} - {message}")]
    Server { code: i64, message: String },
}

impl transport::ServiceErrorCategory for McpError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Connection(_) | Self::Transport(_) | Self::NotConnected => {
                transport::ErrorCategory::External
            }
            Self::Protocol(_) | Self::InvalidResponse(_) | Self::Server { .. } => {
                transport::ErrorCategory::External
            }
            Self::Timeout => transport::ErrorCategory::Timeout,
            Self::ToolNotFound(_) | Self::ResourceNotFound(_) | Self::PromptNotFound(_) => {
                transport::ErrorCategory::Validation
            }
            Self::Io(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::Http(_) => transport::ErrorCategory::External,
        }
    }
}

/// MCP Result 类型
pub type McpResult<T> = Result<T, McpError>;

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
