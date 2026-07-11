use transport::commands::{ImportDataSourceRequest, ImportTargetPlatformDto};

#[test]
fn legacy_macos_platform_deserializes_as_unsupported() {
    let request: ImportDataSourceRequest =
        serde_json::from_str(r#"{"sourcePath":"D:/evidence/disk.e01","platform":"macos"}"#)
            .expect("legacy macOS platform must remain parseable");

    assert_eq!(request.platform, Some(ImportTargetPlatformDto::Unsupported));
}

#[test]
fn unknown_platform_deserializes_as_unsupported() {
    let request: ImportDataSourceRequest =
        serde_json::from_str(r#"{"sourcePath":"D:/evidence/disk.e01","platform":"solaris"}"#)
            .expect("unknown platforms must reach the typed unsupported branch");

    assert_eq!(request.platform, Some(ImportTargetPlatformDto::Unsupported));
}

#[test]
fn supported_platform_contract_is_unchanged() {
    for (wire_value, expected) in [
        ("windows", ImportTargetPlatformDto::Windows),
        ("linux", ImportTargetPlatformDto::Linux),
        ("unknown", ImportTargetPlatformDto::Unknown),
    ] {
        let raw = format!(r#"{{"sourcePath":"D:/evidence/disk.e01","platform":"{wire_value}"}}"#);
        let request: ImportDataSourceRequest =
            serde_json::from_str(&raw).expect("supported platform must deserialize");

        assert_eq!(request.platform, Some(expected));
    }
}
