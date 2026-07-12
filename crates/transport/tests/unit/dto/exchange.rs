use super::*;

#[test]
fn stix_export_request_serializes_camel_case() {
    let req = StixExportRequestDto {
        artifact_type_filter: Some("LNK".to_string()),
    };
    let value = serde_json::to_value(req).unwrap();
    assert_eq!(value["artifactTypeFilter"], "LNK");
    assert!(value.get("artifact_type_filter").is_none());
}

#[test]
fn stix_export_request_skips_optional_filter() {
    let req = StixExportRequestDto {
        artifact_type_filter: None,
    };
    let value = serde_json::to_value(req).unwrap();
    assert!(value.get("artifactTypeFilter").is_none());
}

#[test]
fn stix_export_result_serializes_camel_case() {
    let result = StixExportResultDto {
        json: "{}".to_string(),
        object_count: 5,
        indicator_count: 2,
        observed_data_count: 2,
        relationship_count: 1,
        generated_at: "2026-06-17T00:00:00Z".to_string(),
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["objectCount"], 5);
    assert_eq!(value["indicatorCount"], 2);
    assert_eq!(value["observedDataCount"], 2);
    assert_eq!(value["relationshipCount"], 1);
    assert_eq!(value["generatedAt"], "2026-06-17T00:00:00Z");
    assert!(value.get("object_count").is_none());
}
