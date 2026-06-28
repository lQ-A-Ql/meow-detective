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
mod tests {
    use super::*;

    #[test]
    fn test_viewer_handle_dto_serialization() {
        let dto = ViewerHandleDto {
            handle_id: "file:123".to_string(),
            size: 1024,
            mime: Some("text/plain".to_string()),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("handleId"));
        assert!(json.contains("file:123"));
        assert!(json.contains("1024"));
    }

    #[test]
    fn test_viewer_handle_dto_no_mime() {
        let dto = ViewerHandleDto {
            handle_id: "file:123".to_string(),
            size: 1024,
            mime: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("mime"));
    }

    #[test]
    fn test_text_preview_dto_serialization() {
        let dto = TextPreviewDto {
            content: "Hello World".to_string(),
            encoding: "UTF-8".to_string(),
            is_truncated: false,
            line_count: 1,
            is_binary: false,
            language: Some("plaintext".to_string()),
            hex_dump: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("Hello World"));
        assert!(json.contains("UTF-8"));
    }

    #[test]
    fn test_image_preview_dto_serialization() {
        let dto = ImagePreviewDto {
            data_url: "data:image/png;base64,...".to_string(),
            mime_type: "image/png".to_string(),
            width: 1920,
            height: 1080,
            size: 1024000,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("data:image/png"));
        assert!(json.contains("1920"));
    }

    #[test]
    fn test_media_url_dto_serialization() {
        let dto = MediaUrlDto {
            mode: MediaPreviewModeDto::Inline,
            url: Some("data:video/mp4;base64,AAAA".to_string()),
            handle_id: None,
            mime_type: "video/mp4".to_string(),
            size: 10240000,
            can_read_ranges: false,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("data:video/mp4"));
        assert!(json.contains("canReadRanges"));
        assert!(json.contains("\"mode\":\"inline\""));
    }

    #[test]
    fn test_media_url_protocol_serialization() {
        let dto = MediaUrlDto {
            mode: MediaPreviewModeDto::Protocol,
            url: Some("evidence-media://handle/file%3Aabc".to_string()),
            handle_id: Some("file:abc".to_string()),
            mime_type: "video/mp4".to_string(),
            size: 10240000,
            can_read_ranges: true,
        };
        let json = serde_json::to_string(&dto).unwrap();

        assert!(json.contains("\"mode\":\"protocol\""));
        assert!(json.contains("evidence-media://handle/file%3Aabc"));
        assert!(json.contains("handleId"));
    }

    #[test]
    fn test_viewer_range_request_deserialization() {
        let json = r#"{"handleId":"file:123","offset":0,"length":1024}"#;
        let dto: ViewerRangeRequestDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.handle_id, "file:123");
        assert_eq!(dto.offset, 0);
        assert_eq!(dto.length, 1024);
    }

    #[test]
    fn viewer_range_request_clamps_oversized_length() {
        let mut dto = ViewerRangeRequestDto {
            handle_id: "file:123".to_string(),
            offset: 0,
            length: u32::MAX,
        };

        dto.validate().unwrap();

        assert_eq!(dto.length, MAX_VIEWER_RANGE_LENGTH);
    }

    #[test]
    fn viewer_range_request_rejects_empty_handle() {
        let mut dto = ViewerRangeRequestDto {
            handle_id: " ".to_string(),
            offset: 0,
            length: 1024,
        };

        assert!(dto.validate().is_err());
    }

    #[test]
    fn viewer_range_request_rejects_zero_length() {
        let mut dto = ViewerRangeRequestDto {
            handle_id: "file:123".to_string(),
            offset: 0,
            length: 0,
        };

        assert!(dto.validate().is_err());
    }

    #[test]
    fn media_range_request_clamps_oversized_length() {
        let mut dto = MediaRangeRequestDto {
            handle_id: "file:123".to_string(),
            offset: 0,
            length: u32::MAX,
        };

        dto.validate().unwrap();

        assert_eq!(dto.length, MAX_VIEWER_RANGE_LENGTH);
    }

    #[test]
    fn media_range_request_rejects_zero_length() {
        let mut dto = MediaRangeRequestDto {
            handle_id: "file:123".to_string(),
            offset: 0,
            length: 0,
        };

        assert!(dto.validate().is_err());
    }
}
