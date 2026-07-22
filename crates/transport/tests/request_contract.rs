use transport::commands::{
    AppSettingsDto, ClassifyFilesRequest, ExportDeletedRecoveryRequest, ExportScopeDto,
    ExtractFileRequest, FileSortDirectionDto, FileSortKeyDto, GetArtifactByIdRequest,
    GetEvtxEventSummaryRequest, GetFileChildrenRequest, GetFileJumpContextRequest,
    GetFileRowsRequest, GetFileTreeRequest, GetTimelineEventByIdRequest, GetTimelineRequest,
    ImportDataSourceRequest, ImportSourceKindDto, ImportTargetPlatformDto,
    ListDeletedRecoveriesRequest, ReadDeletedRecoveryRangeRequest, RunDeletedRecoveryRequest,
};
use transport::dto::EvtxEventViewDto;

#[test]
fn classify_files_request_deserializes_sample_size() {
    let request: ClassifyFilesRequest =
        serde_json::from_str(r#"{"dataSourceId":"ds-1","sampleSize":1000}"#)
            .expect("classification request must deserialize");

    assert_eq!(request.data_source_id, "ds-1");
    assert_eq!(request.sample_size, Some(1000));
}

#[test]
fn evtx_summary_request_deserializes_view_and_bounds_page_size() {
    let mut request: GetEvtxEventSummaryRequest = serde_json::from_str(
        r#"{"dataSourceId":"ds-1","view":"process","offset":500,"limit":999999}"#,
    )
    .unwrap();

    request.validate().unwrap();
    assert_eq!(request.view, Some(EvtxEventViewDto::Process));
    assert_eq!(request.offset, 500);
    assert_eq!(request.limit, 500);
}

#[test]
fn import_source_rejects_reserved_device_names() {
    let request = ImportDataSourceRequest {
        source_path: "CON".to_string(),
        source_kind: Default::default(),
        platform: ImportTargetPlatformDto::Windows,
        profile: None,
    };

    assert!(request.validate().is_err());
}

#[test]
fn extract_file_request_rejects_device_destination() {
    let request = ExtractFileRequest {
        file_id: "file-1".to_string(),
        destination_path: r"\\.\PhysicalDrive0".to_string(),
        overwrite: false,
    };

    assert!(request.validate().is_err());
}

#[test]
fn import_source_rejects_windows_device_paths() {
    let request = ImportDataSourceRequest {
        source_path: r"\\.\PhysicalDrive0".to_string(),
        source_kind: Default::default(),
        platform: ImportTargetPlatformDto::Windows,
        profile: None,
    };

    assert!(request.validate().is_err());
}

#[test]
fn import_source_rejects_extended_length_paths() {
    let request = ImportDataSourceRequest {
        source_path: r"\\?\C:\evidence.E01".to_string(),
        source_kind: Default::default(),
        platform: ImportTargetPlatformDto::Windows,
        profile: None,
    };

    assert!(request.validate().is_err());
}

#[test]
fn import_source_accepts_required_platform_and_optional_profile_contract() {
    let request: ImportDataSourceRequest = serde_json::from_str(
        r#"{"sourcePath":"C:/evidence/linux.raw","platform":"linux","profile":"ubuntu-server"}"#,
    )
    .unwrap();

    assert_eq!(request.platform, ImportTargetPlatformDto::Linux);
    assert_eq!(request.profile.as_deref(), Some("ubuntu-server"));
    assert!(request.validate().is_ok());

    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["sourcePath"], "C:/evidence/linux.raw");
    assert_eq!(value["platform"], "linux");
    assert_eq!(value["profile"], "ubuntu-server");
    assert!(value.get("sourceKind").is_none());
    assert!(value.get("source_path").is_none());
}

#[test]
fn import_source_accepts_linux_cluster_source_kind_contract() {
    let request: ImportDataSourceRequest = serde_json::from_str(
        r#"{"sourcePath":"D:/cluster","sourceKind":"linuxCluster","platform":"linux"}"#,
    )
    .unwrap();

    assert_eq!(request.source_kind, ImportSourceKindDto::LinuxCluster);
    assert_eq!(request.platform, ImportTargetPlatformDto::Linux);
    assert!(request.validate().is_ok());

    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["sourceKind"], "linuxCluster");
}

#[test]
fn import_source_rejects_linux_cluster_without_linux_platform() {
    let request: ImportDataSourceRequest = serde_json::from_str(
        r#"{"sourcePath":"D:/cluster","sourceKind":"linuxCluster","platform":"windows"}"#,
    )
    .unwrap();

    assert_eq!(
        request.validate().unwrap_err(),
        "linuxCluster imports must use platform linux"
    );
}

#[test]
fn timeline_request_clamps_limit() {
    let mut request = GetTimelineRequest {
        limit: u32::MAX,
        ..Default::default()
    };

    request.validate().unwrap();

    assert_eq!(request.limit, 500);
}

#[test]
fn timeline_request_rejects_reversed_time_range() {
    let mut request = GetTimelineRequest {
        time_start: Some("2026-02-02T00:00:00Z".to_string()),
        time_end: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };

    assert!(request.validate().is_err());
}

#[test]
fn app_settings_rejects_missing_case_root() {
    let settings = AppSettingsDto {
        case_root: "Z:/definitely/missing/forensics/path".to_string(),
        ..Default::default()
    };

    assert!(settings.validate().is_err());
}

#[test]
fn app_settings_rejects_zero_import_workers() {
    let settings = AppSettingsDto {
        case_root: std::env::temp_dir().display().to_string(),
        max_import_workers: Some(0),
        ..Default::default()
    };

    assert!(settings.validate().is_err());
}

#[test]
fn app_settings_rejects_zero_analysis_workers() {
    let settings = AppSettingsDto {
        case_root: std::env::temp_dir().display().to_string(),
        max_analysis_workers: Some(0),
        ..Default::default()
    };

    assert!(settings.validate().is_err());
}

#[test]
fn app_settings_defaults_to_metadata_only_import_analysis() {
    let settings: AppSettingsDto = serde_json::from_str(&format!(
        r#"{{"caseRoot":"{}","imageSearchPaths":[],"devEventTrace":false}}"#,
        std::env::temp_dir()
            .display()
            .to_string()
            .replace('\\', "\\\\")
    ))
    .unwrap();

    assert_eq!(settings.import_analysis_mode, "metadataOnly");
    settings.validate().unwrap();
}

#[test]
fn app_settings_rejects_unknown_import_analysis_mode() {
    let settings = AppSettingsDto {
        case_root: std::env::temp_dir().display().to_string(),
        import_analysis_mode: "deepMagic".to_string(),
        ..Default::default()
    };

    assert!(settings.validate().is_err());
}

#[test]
fn export_scope_defaults_enable_existing_sections_only() {
    let scope: ExportScopeDto = serde_json::from_str("{}").unwrap();

    assert!(scope.file_system_metadata);
    assert!(scope.registry);
    assert!(scope.full_timeline);
    assert!(!scope.raw_file_extraction);
    assert!(!scope.overwrite);
}

#[test]
fn file_rows_request_deserializes_show_hidden_camel_case() {
    let request: GetFileRowsRequest =
        serde_json::from_str(r#"{"parentId":"root","offset":10,"limit":50,"showHidden":true}"#)
            .unwrap();

    assert_eq!(request.parent_id.as_deref(), Some("root"));
    assert_eq!(request.offset, 10);
    assert_eq!(request.limit, 50);
    assert!(request.show_hidden);
}

#[test]
fn file_tree_request_defaults_show_hidden_to_false() {
    let request: GetFileTreeRequest = serde_json::from_str("{}").unwrap();
    assert!(!request.show_hidden);
}

#[test]
fn file_children_request_deserializes_show_hidden_camel_case() {
    let request: GetFileChildrenRequest =
        serde_json::from_str(r#"{"parentId":"root","showHidden":true}"#).unwrap();

    assert_eq!(request.parent_id, "root");
    assert!(request.show_hidden);
}

#[test]
fn file_jump_context_request_deserializes_sort_and_limit() {
    let request: GetFileJumpContextRequest = serde_json::from_str(
        r#"{"fileId":"file-1","showHidden":true,"pageLimit":250,"sortKey":"modifiedAt","sortDirection":"desc"}"#,
    )
    .unwrap();

    assert_eq!(request.file_id, "file-1");
    assert!(request.show_hidden);
    assert_eq!(request.page_limit, 250);
    assert_eq!(request.sort_key, FileSortKeyDto::ModifiedAt);
    assert_eq!(request.sort_direction, FileSortDirectionDto::Desc);
}

#[test]
fn deleted_recovery_requests_validate_source_and_bound_page_size() {
    let run: RunDeletedRecoveryRequest =
        serde_json::from_str(r#"{"dataSourceId":"source-1","partitionIndex":2}"#).unwrap();
    assert!(run.validate().is_ok());

    let mut list: ListDeletedRecoveriesRequest = serde_json::from_str(
        r#"{"dataSourceId":"source-1","partitionIndex":2,"offset":100,"limit":999999}"#,
    )
    .unwrap();
    list.validate().unwrap();
    assert_eq!(list.limit, 500);

    let invalid = RunDeletedRecoveryRequest {
        data_source_id: "../source".to_string(),
        partition_index: None,
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn deleted_recovery_content_requests_validate_identity_range_and_destination() {
    let recovery_id = format!("recovery:{}", "a".repeat(64));
    let mut read = ReadDeletedRecoveryRangeRequest {
        data_source_id: "source-1".to_string(),
        recovery_id: recovery_id.clone(),
        offset: 0,
        length: u32::MAX,
    };
    read.validate().unwrap();
    assert_eq!(read.length, 1024 * 1024);

    let export = ExportDeletedRecoveryRequest {
        data_source_id: "source-1".to_string(),
        recovery_id,
        destination_path: "D:/exports/recovered.bin".to_string(),
        overwrite: false,
    };
    export.validate().unwrap();

    read.recovery_id = "candidate-1".to_string();
    assert!(read.validate().is_err());
}

#[test]
fn artifact_by_id_request_requires_id() {
    assert!(GetArtifactByIdRequest {
        artifact_id: String::new(),
    }
    .validate()
    .is_err());
}

#[test]
fn timeline_event_by_id_request_requires_id() {
    assert!(GetTimelineEventByIdRequest {
        event_id: String::new(),
    }
    .validate()
    .is_err());
}
