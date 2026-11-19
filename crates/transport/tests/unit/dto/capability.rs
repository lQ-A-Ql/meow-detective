use super::*;

#[test]
fn capability_dto_serializes_camel_case_and_skips_options() {
    let dto = SourceCapabilityDto {
        data_source_id: "ds-1".to_string(),
        capability: CapabilityKindDto::PlatformFacts,
        state: CapabilityStateDto::Ready,
        revision: 3,
        progress: Some(CapabilityProgressDto {
            done: 12,
            total: Some(40),
            phase: Some("extracting".to_string()),
        }),
        detail: Some("derived from catalog".to_string()),
        error_code: None,
        derived: true,
        updated_at: "2026-11-19T00:00:00Z".to_string(),
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["dataSourceId"], "ds-1");
    assert_eq!(value["capability"], "platformFacts");
    assert_eq!(value["state"], "ready");
    assert_eq!(value["revision"], 3);
    assert_eq!(value["progress"]["done"], 12);
    assert_eq!(value["progress"]["total"], 40);
    assert_eq!(value["progress"]["phase"], "extracting");
    assert_eq!(value["detail"], "derived from catalog");
    assert!(value.get("errorCode").is_none());
    assert_eq!(value["derived"], true);
    assert!(value.get("data_source_id").is_none());

    let round: SourceCapabilityDto = serde_json::from_value(value).unwrap();
    assert_eq!(round.revision, 3);
    assert!(round.derived);
}

#[test]
fn capability_dto_skips_missing_optional_fields() {
    let dto = SourceCapabilityDto {
        data_source_id: "ds-2".to_string(),
        capability: CapabilityKindDto::Timeline,
        state: CapabilityStateDto::Pending,
        revision: 0,
        progress: None,
        detail: None,
        error_code: None,
        derived: false,
        updated_at: "2026-11-19T00:00:00Z".to_string(),
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert!(value.get("progress").is_none());
    assert!(value.get("detail").is_none());
    assert!(value.get("errorCode").is_none());
    assert_eq!(value["state"], "pending");
}

#[test]
fn capability_matrix_defaults_to_empty_sources() {
    let matrix = CapabilityMatrixDto::default();
    assert!(matrix.sources.is_empty());
    let value = serde_json::to_value(&matrix).unwrap();
    assert_eq!(value["sources"].as_array().unwrap().len(), 0);
}
