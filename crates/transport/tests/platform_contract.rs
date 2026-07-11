use transport::commands::{ImportDataSourceRequest, ImportTargetPlatformDto};

#[test]
fn import_request_requires_platform() {
    let error =
        serde_json::from_str::<ImportDataSourceRequest>(r#"{"sourcePath":"D:/evidence/disk.e01"}"#)
            .expect_err("platform is a required transport field");

    assert!(error.to_string().contains("platform"));
}

#[test]
fn legacy_macos_platform_deserializes_as_unsupported() {
    let request: ImportDataSourceRequest =
        serde_json::from_str(r#"{"sourcePath":"D:/evidence/disk.e01","platform":"macos"}"#)
            .expect("legacy macOS platform must remain parseable");

    assert_eq!(request.platform, ImportTargetPlatformDto::Unsupported);
}

#[test]
fn unknown_platform_deserializes_as_unsupported() {
    let request: ImportDataSourceRequest =
        serde_json::from_str(r#"{"sourcePath":"D:/evidence/disk.e01","platform":"solaris"}"#)
            .expect("unknown platforms must reach the typed unsupported branch");

    assert_eq!(request.platform, ImportTargetPlatformDto::Unsupported);
}

#[test]
fn supported_platform_contract_is_unchanged() {
    for (wire_value, expected) in [
        ("windows", ImportTargetPlatformDto::Windows),
        ("linux", ImportTargetPlatformDto::Linux),
    ] {
        let raw = format!(r#"{{"sourcePath":"D:/evidence/disk.e01","platform":"{wire_value}"}}"#);
        let request: ImportDataSourceRequest =
            serde_json::from_str(&raw).expect("supported platform must deserialize");

        assert_eq!(request.platform, expected);
    }
}

#[test]
fn unknown_platform_deserializes_to_the_unsupported_branch() {
    let request: ImportDataSourceRequest =
        serde_json::from_str(r#"{"sourcePath":"D:/evidence/disk.e01","platform":"unknown"}"#)
            .expect("retired unknown value must remain parseable");

    assert_eq!(request.platform, ImportTargetPlatformDto::Unsupported);
}
