use super::*;

#[test]
fn api_error_dto_serializes_suggestion_in_camel_case() {
    let err = ApiErrorDto::new("IMPORT_NEEDED", "path reconstruction failed", true)
        .with_suggestion("建议重新导入 E01 镜像以重建完整路径");
    let value = serde_json::to_value(err).expect("serialize ApiErrorDto");
    assert_eq!(value["suggestion"], "建议重新导入 E01 镜像以重建完整路径");
    assert!(value.get("category").is_none());
    assert!(value.get("details").is_none());
}

#[test]
fn api_error_dto_omits_suggestion_when_none() {
    let err = ApiErrorDto::new("INTERNAL", "something failed", false);
    let value = serde_json::to_value(err).expect("serialize ApiErrorDto");
    assert!(value.get("suggestion").is_none());
    assert_eq!(value["code"], "INTERNAL");
    assert_eq!(value["recoverable"], false);
}
