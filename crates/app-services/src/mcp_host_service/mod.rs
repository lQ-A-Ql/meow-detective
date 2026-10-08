//! Read-only tool catalog and use cases shared by desktop UI and MCP clients.
mod arguments;
mod catalog;
mod current_case;
mod data_sources;
mod files;
mod list_files;
mod plugin_modules;
mod queries;
mod read_file;
mod sanitization;

pub use catalog::{tool_catalog, validate_settings};
pub use files::{execute_file_tool, McpHostFileQueryContext};
pub use queries::{execute_tool, McpHostQueryContext};

#[derive(Debug, thiserror::Error)]
pub enum McpHostServiceError {
    #[error("工具名称或参数无效")]
    InvalidInput,
    #[error("未找到指定数据源、插件或文件")]
    NotFound,
    #[error("读取文件失败")]
    File(#[source] crate::file_service::FileServiceError),
    #[error("读取结果未包含文件字节")]
    MissingBytes,
    #[error("文件读取不完整，请检查数据源状态")]
    IncompleteRead,
    #[error("该文件的存储格式暂不支持读取")]
    UnsupportedFile,
    #[error("该数据块不是有效的 UTF-8，请使用 auto 或 base64 编码")]
    InvalidUtf8,
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
            Self::InvalidInput | Self::NotFound | Self::InvalidUtf8 => {
                transport::ErrorCategory::Validation
            }
            Self::File(error) => error.category(),
            Self::IncompleteRead => transport::ErrorCategory::Parser,
            Self::UnsupportedFile => transport::ErrorCategory::Unsupported,
            _ => transport::ErrorCategory::Internal,
        }
    }
}
