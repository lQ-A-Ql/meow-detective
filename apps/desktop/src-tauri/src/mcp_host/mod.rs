//! Application-owned loopback MCP server and desktop access to its tools.
mod execution;
mod handler;
mod runtime;

pub use execution::call_tool;
pub use runtime::McpHostRuntime;

#[derive(Debug, thiserror::Error)]
pub enum McpHostError {
    #[error("本机 MCP 状态不可用")]
    State,
    #[error("无法监听本机 MCP 端口 3001，请检查端口是否被占用")]
    Bind(#[source] std::io::Error),
    #[error("无法保存或读取本机 MCP 配置")]
    Io(#[from] std::io::Error),
    #[error("本机 MCP 配置格式无效")]
    Json(#[from] serde_json::Error),
    #[error("本机 MCP 配置超过大小限制")]
    SettingsTooLarge,
    #[error("本机 MCP 工具配置无效")]
    Policy(#[from] app_services::mcp_host_service::McpHostServiceError),
}

impl transport::ServiceErrorCategory for McpHostError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Bind(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::Json(_) | Self::Policy(_) | Self::SettingsTooLarge => {
                transport::ErrorCategory::Validation
            }
            Self::State => transport::ErrorCategory::Internal,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp_host/mod.rs"]
mod tests;
