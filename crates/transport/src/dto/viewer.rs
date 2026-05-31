use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerHandleDto {
    pub handle_id: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRangeRequestDto {
    pub handle_id: String,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRangeResponseDto {
    pub kind: String,
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

/// Text preview DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreviewDto {
    /// Text content
    pub content: String,
    /// Encoding name (UTF-8, GBK, etc.)
    pub encoding: String,
    /// Whether content was truncated
    pub is_truncated: bool,
    /// Number of lines
    pub line_count: usize,
    /// Whether file is binary
    pub is_binary: bool,
    /// Programming language (for syntax highlighting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Image preview DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreviewDto {
    /// Data URL (data:mime;base64,...)
    pub data_url: String,
    /// MIME type
    pub mime_type: String,
    /// Image width (0 if unknown)
    pub width: u32,
    /// Image height (0 if unknown)
    pub height: u32,
    /// File size in bytes
    pub size: u64,
}

/// Media URL DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaUrlDto {
    /// Media URL
    pub url: String,
    /// MIME type
    pub mime_type: String,
    /// File size in bytes
    pub size: u64,
}
