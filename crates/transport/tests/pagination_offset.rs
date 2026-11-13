use transport::{
    commands::{GetArtifactRowsRequest, GetTimelineRequest},
    paging::{validate_page_offset, OffsetLimitError, MAX_PAGE_OFFSET},
};

#[test]
fn item_offset_bounds_are_inclusive_and_report_typed_errors() {
    for offset in [0, 9_999, MAX_PAGE_OFFSET] {
        assert_eq!(validate_page_offset(offset), Ok(()));
    }
    for offset in [MAX_PAGE_OFFSET + 1, u64::MAX] {
        let error = validate_page_offset(offset).unwrap_err();
        assert_eq!(error, OffsetLimitError { offset });
        assert!(error.to_string().contains("nextCursor"));
        assert!(error.to_string().contains("10000"));
    }
}

#[test]
fn artifact_and_timeline_requests_share_item_offset_bounds() {
    for offset in [0, 9_999, 10_000, 10_001, u64::MAX] {
        let mut artifact: GetArtifactRowsRequest =
            serde_json::from_value(serde_json::json!({"offset":offset,"limit":1})).unwrap();
        let mut timeline: GetTimelineRequest =
            serde_json::from_value(serde_json::json!({"offset":offset,"limit":1})).unwrap();
        let artifact_result = artifact.validate();
        let timeline_result = timeline.validate();
        assert_eq!(artifact_result.is_ok(), offset <= MAX_PAGE_OFFSET);
        assert_eq!(timeline_result, artifact_result);
    }
}

#[test]
fn item_cursors_require_zero_offset() {
    let cursor = transport::paging::encode_opaque_cursor(&serde_json::json!({"page":1})).unwrap();
    let mut artifact = GetArtifactRowsRequest {
        offset: 1,
        limit: 1,
        cursor: Some(cursor.clone()),
        ..Default::default()
    };
    let mut timeline = GetTimelineRequest {
        offset: 1,
        limit: 1,
        cursor: Some(cursor),
        ..Default::default()
    };
    assert_eq!(
        artifact.validate().unwrap_err(),
        "offset must be zero when cursor is provided"
    );
    assert_eq!(
        timeline.validate().unwrap_err(),
        "offset must be zero when cursor is provided"
    );
}
