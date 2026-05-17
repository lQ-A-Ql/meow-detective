use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorDto {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub recoverable: bool,
}

impl ApiErrorDto {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            recoverable,
        }
    }
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("{code}: {message}")]
    Api { code: String, message: String },
}

impl From<ApiErrorDto> for TransportError {
    fn from(value: ApiErrorDto) -> Self {
        Self::Api {
            code: value.code,
            message: value.message,
        }
    }
}
