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
            url: "asset://localhost/path".to_string(),
            mime_type: "video/mp4".to_string(),
            size: 10240000,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("asset://localhost"));
    }

    #[test]
    fn test_viewer_range_request_deserialization() {
        let json = r#"{"handleId":"file:123","offset":0,"length":1024}"#;
        let dto: ViewerRangeRequestDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.handle_id, "file:123");
        assert_eq!(dto.offset, 0);
        assert_eq!(dto.length, 1024);
    }
}
