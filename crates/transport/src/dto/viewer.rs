use serde::{Deserialize, Serialize};

pub const MAX_VIEWER_RANGE_LENGTH: u32 = 1024 * 1024;

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

impl ViewerRangeRequestDto {
    /// Validate and normalize a range request before it reaches evidence readers.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.handle_id.trim().is_empty() {
            return Err("handleId is required".to_string());
        }
        if self.length == 0 {
            return Err("length must be greater than zero".to_string());
        }
        self.length = self.length.min(MAX_VIEWER_RANGE_LENGTH);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRangeResponseDto {
    pub kind: String,
    /// Compatibility field for older hex viewers. Bytes-only responses leave this empty.
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Raw bytes for hex preview (single-response payload, currently up to 1MB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<Vec<u8>>,
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
    /// Hex dump of first 64KB for binary files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex_dump: Option<String>,
}

/// A titled text section inside a document preview (page, sheet, table, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSectionDto {
    /// Section title (e.g. "Page 1", "Sheet: Summary", "Table: logins")
    pub title: String,
    /// Bounded text lines of the section
    pub lines: Vec<String>,
}

/// Structured preview for document-like files (PDF, Office Open XML, SQLite).
///
/// This is a bounded text extraction, not a layout renderer; binary payloads
/// and images are never inlined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPreviewDto {
    /// Document kind: pdf | docx | xlsx | pptx | sqlite
    pub kind: String,
    /// Short summary (e.g. "12 pages", "3 sheets, 2 tables read")
    pub summary: String,
    /// Bounded sections
    pub sections: Vec<DocumentSectionDto>,
    /// Whether content was truncated by the preview bounds
    pub truncated: bool,
    /// Non-fatal per-part warnings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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
    /// Preview delivery mode.
    pub mode: MediaPreviewModeDto,
    /// Media URL. Present for bounded inline previews and scoped protocol previews.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Opaque viewer handle for command-scoped range reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_id: Option<String>,
    /// MIME type
    pub mime_type: String,
    /// File size in bytes
    pub size: u64,
    /// Whether the frontend can request bounded byte ranges for this media.
    pub can_read_ranges: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaPreviewModeDto {
    Inline,
    Protocol,
    RangeFallback,
}

/// Raw media byte range request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRangeRequestDto {
    pub handle_id: String,
    pub offset: u64,
    pub length: u32,
}

impl MediaRangeRequestDto {
    /// Validate and normalize a media byte range request.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.handle_id.trim().is_empty() {
            return Err("handleId is required".to_string());
        }
        if self.length == 0 {
            return Err("length must be greater than zero".to_string());
        }
        self.length = self.length.min(MAX_VIEWER_RANGE_LENGTH);
        Ok(())
    }
}

/// Raw media byte range response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRangeResponseDto {
    pub offset: u64,
    pub bytes_base64: String,
    pub bytes_read: u32,
    pub eof: bool,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/viewer.rs"]
mod tests;
