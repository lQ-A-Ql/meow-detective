use super::*;

#[test]
fn transport_platforms_map_to_domain_platforms() {
    let cases = [
        (
            ImportTargetPlatformDto::Windows,
            DataSourcePlatform::Windows,
        ),
        (ImportTargetPlatformDto::Linux, DataSourcePlatform::Linux),
    ];

    for (transport, domain) in cases {
        assert_eq!(
            import_platform_from_dto(transport).expect("supported platform"),
            domain
        );
    }
}

#[test]
fn missing_platform_fails_dto_deserialization() {
    let error = serde_json::from_str::<ImportDataSourceRequest>(
        r#"{"sourcePath":"Z:/path-that-must-not-exist/missing-platform.E01"}"#,
    )
    .expect_err("platform must be explicit");

    assert!(error.to_string().contains("missing field `platform`"));
}

#[test]
fn retired_transport_platform_fails_before_source_access() {
    let request = ImportDataSourceRequest {
        source_path: "Z:/path-that-must-not-exist/retired-platform.E01".to_string(),
        source_kind: ImportSourceKindDto::Auto,
        platform: ImportTargetPlatformDto::Unsupported,
        profile: None,
    };
    let error = validate_import_request(&request).expect_err("retired platform must fail closed");

    assert_eq!(error.code, "UNSUPPORTED");
    assert_eq!(error.category, "unsupported");
}
