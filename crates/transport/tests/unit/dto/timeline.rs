use super::*;

#[test]
fn timeline_event_dto_serializes_optional_provenance_as_camel_case() {
    let dto = TimelineEventDto {
        id: "timeline-1".to_string(),
        source_object_id: "file-1".to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        ts: "2026-06-04T00:00:00Z".to_string(),
        title: "Modified".to_string(),
        description: "description".to_string(),
        parser_id: Some("timeline.macb".to_string()),
        parser_version: Some("1.0.0".to_string()),
        confidence: Some(0.8),
        source_attribution: Some("$STANDARD_INFORMATION".to_string()),
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["sourceObjectId"], "file-1");
    assert_eq!(value["parserId"], "timeline.macb");
    assert_eq!(value["parserVersion"], "1.0.0");
    assert!((value["confidence"].as_f64().unwrap() - 0.8).abs() < 0.000001);
    assert_eq!(value["sourceAttribution"], "$STANDARD_INFORMATION");
    assert!(value.get("parser_id").is_none());
    assert!(value.get("source_attribution").is_none());
}

#[test]
fn timeline_event_dto_skips_missing_optional_provenance() {
    let dto = TimelineEventDto {
        id: "timeline-1".to_string(),
        source_object_id: "file-1".to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        ts: "2026-06-04T00:00:00Z".to_string(),
        title: "Modified".to_string(),
        description: "description".to_string(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert!(value.get("parserId").is_none());
    assert!(value.get("parserVersion").is_none());
    assert!(value.get("confidence").is_none());
    assert!(value.get("sourceAttribution").is_none());
}
