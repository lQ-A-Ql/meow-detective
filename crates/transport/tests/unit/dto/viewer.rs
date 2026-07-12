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
