use super::*;

#[test]
fn test_select_strategy_small() {
    assert_eq!(select_strategy(100, 1024), ImportStrategy::Sequential);
}

#[test]
fn test_select_strategy_medium() {
    assert_eq!(
        select_strategy(5000, 1024),
        ImportStrategy::Parallel { workers: 4 }
    );
}

#[test]
fn test_select_strategy_large_file() {
    assert_eq!(select_strategy(100, 500_000_000), ImportStrategy::Streaming);
}

#[test]
fn test_estimate_files() {
    assert_eq!(estimate_files_from_size(100_000), 10);
    assert_eq!(estimate_files_from_size(1_000_000), 100);
}

#[test]
fn test_pre_check_nonexistent() {
    let result = pre_import_check(Path::new("/nonexistent"), &DataSourceKind::LogicalDirectory);
    assert!(!result.errors.is_empty());
}

#[test]
fn pre_check_rejects_ceph_rbd_before_filesystem_access() {
    let result = pre_import_check(
        Path::new("C:/path-that-must-not-be-opened"),
        &DataSourceKind::CephRbd,
    );

    assert_eq!(
        result.errors,
        vec!["Ceph RBD derived data sources are not ordinary import sources"]
    );
    assert_eq!(result.plan.total_files, 0);
    assert_eq!(result.plan.total_size, 0);
}

#[test]
fn test_import_plan_time_estimate() {
    let plan = ImportPlan::new(ImportStrategy::Sequential, 1000, 1024 * 1024);
    assert!(plan.estimated_time_secs > 0);
}

#[test]
fn test_import_plan_memory_estimate() {
    let plan = ImportPlan::new(
        ImportStrategy::Parallel { workers: 4 },
        10000,
        1024 * 1024 * 100,
    );
    assert!(plan.estimated_memory_bytes > 0);
}

#[test]
fn import_source_config_classifies_logical_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("logical-evidence");
    std::fs::create_dir(&evidence_dir).unwrap();
    let config = prepare_import_source_config(
        &evidence_dir.display().to_string(),
        DataSourcePlatform::Windows,
        None,
    )
    .unwrap();

    assert_eq!(config.source_path, evidence_dir);
    assert_eq!(config.source_name, "logical-evidence");
    assert_eq!(config.kind, DataSourceKind::LogicalDirectory);
    assert_eq!(config.mode, ImportSourceMode::LogicalDirectory);
    assert!(!config.is_image_backed());
    assert_eq!(config.staging_kind(), None);
}

#[test]
fn import_source_config_preserves_required_platform_and_optional_profile() {
    let tmp = tempfile::TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("linux-logical");
    std::fs::create_dir(&evidence_dir).unwrap();
    let config = prepare_import_source_config(
        &evidence_dir.display().to_string(),
        DataSourcePlatform::Linux,
        Some("ubuntu-server".to_string()),
    )
    .unwrap();

    assert_eq!(config.platform, DataSourcePlatform::Linux);
    assert_eq!(config.profile.as_deref(), Some("ubuntu-server"));
    assert_eq!(config.source_path, evidence_dir);
}

#[test]
fn import_source_config_classifies_raw_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = tmp.path().join("disk.raw");
    std::fs::write(&source, b"not an e01 image").unwrap();
    let config = prepare_import_source_config(
        &source.display().to_string(),
        DataSourcePlatform::Windows,
        None,
    )
    .unwrap();

    assert_eq!(config.source_path, source);
    assert_eq!(config.source_name, "disk.raw");
    assert_eq!(config.kind, DataSourceKind::Raw);
    assert_eq!(
        config.mode,
        ImportSourceMode::Image {
            staging_kind: "Raw"
        }
    );
    assert!(config.is_image_backed());
    assert_eq!(config.staging_kind(), Some("Raw"));
}

#[test]
fn import_source_config_classifies_e01_by_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = tmp.path().join("capture.E01");
    std::fs::write(&source, b"short").unwrap();
    let config = prepare_import_source_config(
        &source.display().to_string(),
        DataSourcePlatform::Windows,
        None,
    )
    .unwrap();

    assert_eq!(config.source_name, "capture.E01");
    assert_eq!(config.kind, DataSourceKind::E01);
    assert_eq!(
        config.mode,
        ImportSourceMode::Image {
            staging_kind: "E01"
        }
    );
    assert_eq!(config.staging_kind(), Some("E01"));
}

#[test]
fn import_source_config_classifies_e01_by_magic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = tmp.path().join("capture.bin");
    std::fs::write(&source, b"EVF\x09\x0d\x0a\xff\x00payload").unwrap();
    let config = prepare_import_source_config(
        &source.display().to_string(),
        DataSourcePlatform::Windows,
        None,
    )
    .unwrap();

    assert_eq!(config.source_name, "capture.bin");
    assert_eq!(config.kind, DataSourceKind::E01);
    assert_eq!(config.staging_kind(), Some("E01"));
}

#[test]
fn import_source_config_rejects_missing_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source = tmp.path().join("missing.raw").display().to_string();
    let error =
        prepare_import_source_config(&source, DataSourcePlatform::Windows, None).unwrap_err();

    assert!(matches!(
        error,
        ImportSourceConfigError::MissingOrInaccessibleSource
    ));
    assert!(error.is_invalid_input());
    assert_eq!(
        error.to_string(),
        "sourcePath must exist and be accessible before import"
    );
}
