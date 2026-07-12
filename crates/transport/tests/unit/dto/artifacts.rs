use super::*;

#[test]
fn artifact_row_dto_serializes_optional_provenance_as_camel_case() {
    let dto = ArtifactRowDto {
        id: "artifact-1".to_string(),
        artifact_type: "Prefetch".to_string(),
        title: "Prefetch artifact".to_string(),
        summary: "summary".to_string(),
        source_object_id: Some("file-1".to_string()),
        extractor_id: Some("prefetch".to_string()),
        extractor_version: Some("1.0.0".to_string()),
        confidence: Some(0.95),
        source_attribution: Some("Windows/Prefetch/CMD.EXE.pf".to_string()),
        created_at: "2026-06-04T00:00:00Z".to_string(),
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["extractorId"], "prefetch");
    assert_eq!(value["extractorVersion"], "1.0.0");
    assert!((value["confidence"].as_f64().unwrap() - 0.95).abs() < 0.000001);
    assert_eq!(value["sourceAttribution"], "Windows/Prefetch/CMD.EXE.pf");
    assert!(value.get("extractor_id").is_none());
    assert!(value.get("source_attribution").is_none());
}

#[test]
fn artifact_row_dto_skips_missing_optional_provenance() {
    let dto = ArtifactRowDto {
        id: "artifact-1".to_string(),
        artifact_type: "Prefetch".to_string(),
        title: "Prefetch artifact".to_string(),
        summary: "summary".to_string(),
        source_object_id: None,
        extractor_id: None,
        extractor_version: None,
        confidence: None,
        source_attribution: None,
        created_at: "2026-06-04T00:00:00Z".to_string(),
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert!(value.get("sourceObjectId").is_none());
    assert!(value.get("extractorId").is_none());
    assert!(value.get("extractorVersion").is_none());
    assert!(value.get("confidence").is_none());
    assert!(value.get("sourceAttribution").is_none());
}
