use super::*;
use serde::{Deserialize, Serialize};

#[test]
fn page_request_clamps_unbounded_limit() {
    let mut request = PageRequest {
        offset: 0,
        limit: u32::MAX,
    };

    request.clamp();

    assert_eq!(request.limit, PageRequest::MAX_LIMIT);
}

#[test]
fn page_request_replaces_zero_with_default() {
    let mut request = PageRequest {
        offset: 0,
        limit: 0,
    };

    request.clamp();

    assert_eq!(request.limit, PageRequest::DEFAULT_LIMIT);
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CursorFixture {
    source_id: String,
    consumed: u64,
}

#[test]
fn opaque_cursor_round_trips_url_safe_payload() {
    let payload = CursorFixture {
        source_id: "source-a".to_string(),
        consumed: 42,
    };

    let cursor = encode_opaque_cursor(&payload).unwrap();
    let decoded: CursorFixture = decode_opaque_cursor(&cursor).unwrap();

    assert_eq!(decoded, payload);
    assert!(cursor.starts_with("v1."));
    assert!(cursor
        .chars()
        .all(|character| !matches!(character, '+' | '/' | '=')));
}

#[test]
fn opaque_cursor_rejects_tampering_and_oversized_input() {
    let mut cursor = encode_opaque_cursor(&CursorFixture {
        source_id: "source-a".to_string(),
        consumed: 1,
    })
    .unwrap()
    .into_bytes();
    let payload_index = cursor.iter().position(|byte| *byte == b'.').unwrap() + 1;
    cursor[payload_index] = if cursor[payload_index] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let cursor = String::from_utf8(cursor).unwrap();

    assert_eq!(
        decode_opaque_cursor::<CursorFixture>(&cursor).unwrap_err(),
        CursorCodecError::IntegrityMismatch
    );
    assert_eq!(
        validate_opaque_cursor(&"x".repeat(MAX_OPAQUE_CURSOR_LENGTH + 1)).unwrap_err(),
        CursorCodecError::Oversized
    );
}

#[test]
fn page_response_uses_optional_camel_case_cursor() {
    let response = PageResponse {
        total: 1,
        items: vec!["row"],
        next_cursor: Some("v1.payload.digest".to_string()),
    };
    let value = serde_json::to_value(response).unwrap();
    assert_eq!(value["nextCursor"], "v1.payload.digest");

    let without_cursor = PageResponse {
        total: 0,
        items: Vec::<String>::new(),
        next_cursor: None,
    };
    assert!(serde_json::to_value(without_cursor)
        .unwrap()
        .get("nextCursor")
        .is_none());
}
