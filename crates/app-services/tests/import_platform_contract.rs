use app_services::import_precheck::{prepare_import_source_config, ImportSourceConfigError};
use transport::commands::{ImportDataSourceRequest, ImportSourceKindDto, ImportTargetPlatformDto};
use transport::{ErrorCategory, ServiceErrorCategory};

#[test]
fn unsupported_platform_fails_before_inaccessible_source_path_is_read() {
    let request = ImportDataSourceRequest {
        source_path: "Z:/path-that-must-not-exist/retired-platform.E01".to_string(),
        source_kind: ImportSourceKindDto::Auto,
        platform: Some(ImportTargetPlatformDto::Unsupported),
        profile: None,
    };

    let error = prepare_import_source_config(&request)
        .expect_err("retired platform must fail before source-path access");

    assert!(matches!(
        error,
        ImportSourceConfigError::UnsupportedPlatform
    ));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
}
