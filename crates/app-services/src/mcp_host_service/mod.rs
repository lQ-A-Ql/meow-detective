//! Read-only tool catalog and use cases shared by desktop UI and MCP clients.
mod catalog;
mod queries;
mod sanitization;

pub use catalog::{tool_catalog, validate_settings};
pub use queries::{execute_tool, McpHostQueryContext};

#[derive(Debug, thiserror::Error)]
pub enum McpHostServiceError {
    #[error("工具名称或参数无效")]
    InvalidInput,
    #[error("未找到指定数据源或插件")]
    NotFound,
    #[error("读取数据源信息失败")]
    DataSource(#[from] crate::file_service::FileServiceError),
    #[error("读取插件信息失败")]
    Plugin(#[from] crate::analysis_service::AnalysisServiceError),
    #[error("无法生成工具结果")]
    Json(#[from] serde_json::Error),
}

impl transport::ServiceErrorCategory for McpHostServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidInput | Self::NotFound => transport::ErrorCategory::Validation,
            _ => transport::ErrorCategory::Internal,
        }
    }
}
