use super::*;

use std::collections::BTreeMap;
use std::path::Path;

use domain::{
    Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind,
    DataSourcePlatform, DataSourceProvenance, EntryType, FileEntry, FileEntryId, TimelineEvent,
    TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
    timeline_repo::TimelineRepo,
};
use transport::commands::ExportScopeDto;
use transport::dto::{AnalysisParseStatusDto, TimelineEventDto};
use transport::{ErrorCategory, ServiceErrorCategory};

fn setup_case() -> (rusqlite::Connection, tempfile::TempDir, CaseId) {
    let case_conn = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&case_conn).expect("run case migrations");
    let case_root = tempfile::TempDir::new().expect("create case root");
    let case_id = CaseId("case-report-platforms".to_string());
    CaseRepo::new(&case_conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Report platform isolation".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");
    (case_conn, case_root, case_id)
}

fn register_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    id: &str,
    platform: &str,
    import_state: &str,
    file_path: Option<&str>,
) {
    let source = DataSource {
        id: DataSourceId(id.to_string()),
        name: id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(id, Some(platform), None);
    storage.import_state = import_state.to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(case_id, &source, &storage)
        .expect("register source");

    let Some(file_path) = file_path else {
        return;
    };
    let source_conn = persistence_sqlite::open_or_create_source(&crate::source_db::source_db_path(
        case_root, &source.id,
    ))
    .expect("create source database");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(case_id, &source)
        .expect("store source-local metadata");
    let name = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);
    FileRepo::new(&source_conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(format!("{id}-file")),
            parent_id: None,
            data_source_id: source.id,
            path: file_path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(32),
            ext: file_path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .expect("insert source file");
}

#[test]
fn mixed_ready_sources_keep_platform_scoped_analysis_sections() {
    let (case_conn, case_root, case_id) = setup_case();
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "linux-newer",
        "linux",
        "ready",
        Some("/var/log/linux.txt"),
    );
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "windows-older",
        "windows",
        "ready",
        Some("C:/Temp/windows.txt"),
    );

    let analysis = current_analysis_for_case(&case_conn, case_root.path(), &case_id)
        .expect("build mixed-source report analysis");

    let ReportAnalysis::PerSource(sources) = analysis else {
        panic!("case analysis must be grouped by data source");
    };
    assert_eq!(sources.len(), 2);
    let linux = sources
        .iter()
        .find(|source| source.data_source_id.0 == "linux-newer")
        .expect("Linux source section");
    let windows = sources
        .iter()
        .find(|source| source.data_source_id.0 == "windows-older")
        .expect("Windows source section");
    assert_eq!(linux.platform, DataSourcePlatform::Linux);
    assert!(linux.system_info.is_none());
    assert_eq!(windows.platform, DataSourcePlatform::Windows);
    assert_eq!(
        windows
            .system_info
            .as_ref()
            .expect("Windows system information")
            .status,
        AnalysisParseStatusDto::NotParsed
    );
    assert!(sources.iter().all(|source| source
        .classifications
        .iter()
        .all(|item| !item.category.contains(&source.data_source_id.0))));
}

#[test]
fn multiple_windows_sources_each_keep_a_system_information_section() {
    let (case_conn, case_root, case_id) = setup_case();
    for id in ["windows-a", "windows-b"] {
        register_source(
            &case_conn,
            case_root.path(),
            &case_id,
            id,
            "windows",
            "ready",
            Some(&format!("C:/Windows/{id}.txt")),
        );
    }

    let analysis = current_analysis_for_case(&case_conn, case_root.path(), &case_id)
        .expect("build multi-Windows report analysis");
    let ReportAnalysis::PerSource(sources) = &analysis else {
        panic!("case analysis must be grouped by data source");
    };

    assert_eq!(sources.len(), 2);
    assert!(sources.iter().all(|source| {
        source.platform == DataSourcePlatform::Windows && source.system_info.is_some()
    }));
    let json =
        crate::report::analysis_json::analysis_json_section(&analysis, &ExportScopeDto::default());
    assert_eq!(
        json["sources"]
            .as_array()
            .expect("source analysis array")
            .iter()
            .filter(|source| !source["systemInfo"].is_null())
            .count(),
        2
    );
}

#[test]
fn linux_only_report_skips_non_ready_sources_and_exposes_registry_warning() {
    let (case_conn, case_root, case_id) = setup_case();
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "linux-ready",
        "linux",
        "ready",
        Some("/etc/hosts.txt"),
    );
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "windows-pending",
        "windows",
        "pending",
        Some("C:/Temp/pending.txt"),
    );
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "retired-failed",
        "macos",
        "failed",
        None,
    );

    let analysis = current_analysis_for_case(&case_conn, case_root.path(), &case_id)
        .expect("build Linux-only report analysis");

    let ReportAnalysis::PerSource(sources) = &analysis else {
        panic!("case analysis must be grouped by data source");
    };
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].data_source_id.0, "linux-ready");
    assert!(sources[0].system_info.is_none());

    let json =
        crate::report::analysis_json::analysis_json_section(&analysis, &ExportScopeDto::default());
    assert!(json["warnings"][0]
        .as_str()
        .is_some_and(|warning| warning.contains("no ready Windows data source")));
}

#[test]
fn ready_unknown_or_retired_platform_fails_before_source_database_open() {
    for platform in ["unknown", "macos"] {
        let (case_conn, case_root, case_id) = setup_case();
        register_source(
            &case_conn,
            case_root.path(),
            &case_id,
            "unsupported-ready",
            platform,
            "ready",
            None,
        );

        let error = current_analysis_for_case(&case_conn, case_root.path(), &case_id)
            .err()
            .expect("unsupported ready platform must fail closed");
        assert!(matches!(error, ReportError::Unsupported(_)));
        assert!(matches!(error.category(), ErrorCategory::Unsupported));
        assert!(!error.to_string().contains("source DB is missing"));
    }
}

#[test]
fn case_report_governance_uses_source_database_correlation() {
    let (case_conn, case_root, case_id) = setup_case();
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "windows-ready",
        "windows",
        "ready",
        Some("Windows/System32/cmd.exe"),
    );
    let source_id = DataSourceId("windows-ready".to_string());
    let source_conn = crate::source_db::open_source_db(case_root.path(), &source_id)
        .expect("open source database");
    ArtifactRepo::new(&source_conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId("prefetch-artifact".to_string()),
                family: "Prefetch".to_string(),
                title: "cmd.exe execution".to_string(),
                summary: "fixture".to_string(),
                source_object_id: Some(FileEntryId("windows-ready-file".to_string())),
                extractor_id: Some("prefetch".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.95),
                source_attribution: Some("fixture".to_string()),
                created_at: chrono::Utc::now(),
                attrs: BTreeMap::new(),
            }],
            &case_id.0,
            &source_id.0,
        )
        .expect("insert source artifact");
    TimelineRepo::new(&source_conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("file-timeline".to_string()),
                source_object_id: "windows-ready-file".to_string(),
                event_type: "FILE_MODIFIED".to_string(),
                timestamp: chrono::Utc::now(),
                title: "cmd.exe modified".to_string(),
                description: "fixture".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.9),
                source_attribution: Some("modified_at".to_string()),
                attrs: BTreeMap::new(),
            }],
            &case_id.0,
        )
        .expect("insert source timeline event");

    let legacy =
        crate::report::current_governance(&case_conn, &case_id.0).expect("build legacy governance");
    let source_aware =
        crate::report::current_governance_for_case(&case_conn, case_root.path(), &case_id.0)
            .expect("build source-aware governance");

    assert_eq!(legacy.snapshot.runtime_signals.correlation_lead_count, 0);
    assert!(source_aware.snapshot.runtime_signals.correlation_lead_count > 0);
}

#[test]
fn multi_source_reports_preserve_global_ids_and_explicit_attribution() {
    let (case_conn, case_root, case_id) = setup_case();
    for (id, platform, path) in [
        ("windows-report", "windows", "C:/Temp/shared.bin"),
        ("linux-report", "linux", "/tmp/shared.bin"),
    ] {
        register_source(
            &case_conn,
            case_root.path(),
            &case_id,
            id,
            platform,
            "ready",
            Some(path),
        );
        seed_source_report_records(case_root.path(), &case_id, id);
    }

    let case = CaseRepo::new(&case_conn)
        .find_by_id(&case_id)
        .expect("load case")
        .expect("case exists");
    let output = tempfile::TempDir::new().expect("create report output");
    let scope = ExportScopeDto::default();

    let json_name = crate::report::generate_json_export_for_case(
        &case_conn,
        &case,
        case_root.path(),
        output.path(),
        &scope,
    )
    .expect("generate source-aware JSON report");
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(output.path().join(json_name)).expect("read JSON report"),
    )
    .expect("parse JSON report");

    for id in ["windows-report", "linux-report"] {
        let artifact_id = format!("ds:{id}:artifact-shared");
        let timeline_id = format!("ds:{id}:timeline-shared");
        let artifact = json["artifacts"]
            .as_array()
            .expect("artifact array")
            .iter()
            .find(|item| item["id"] == artifact_id)
            .expect("source artifact");
        assert_eq!(artifact["dataSourceId"], id);
        assert_eq!(artifact["id"], artifact_id);

        let event = json["timeline_events"]
            .as_array()
            .expect("timeline array")
            .iter()
            .find(|item| item["id"] == timeline_id)
            .expect("source timeline event");
        assert_eq!(event["dataSourceId"], id);
        assert_eq!(event["id"], timeline_id);

        let analysis = json["analysis"]["sources"]
            .as_array()
            .expect("source analysis array")
            .iter()
            .find(|item| item["dataSourceId"] == id)
            .expect("source analysis section");
        assert!(analysis["classifications"]
            .as_array()
            .expect("classification array")
            .iter()
            .all(|item| !item["category"].as_str().unwrap_or_default().contains(id)));
    }

    let html_name = crate::report::generate_html_report_for_case(
        &case_conn,
        &case,
        case_root.path(),
        output.path(),
        &scope,
    )
    .expect("generate source-aware HTML report");
    let html = std::fs::read_to_string(output.path().join(html_name)).expect("read HTML report");
    assert!(html.contains("id=ds:windows-report:artifact-shared"));
    assert!(html.contains("dataSourceId=linux-report"));
    assert!(html.contains("id=ds:linux-report:timeline-shared"));

    let csv_name = crate::report::generate_csv_artifacts_for_case(
        &case_conn,
        &case,
        case_root.path(),
        output.path(),
        &scope,
    )
    .expect("generate source-aware CSV report");
    let csv = std::fs::read_to_string(output.path().join(csv_name)).expect("read CSV report");
    assert!(csv.contains("dataSourceId"));
    assert!(csv.contains("ds:windows-report:artifact-shared"));
    assert!(csv.contains("linux-report"));
}

#[test]
fn public_report_exports_preserve_unsupported_platform_category() {
    let (case_conn, case_root, case_id) = setup_case();
    register_source(
        &case_conn,
        case_root.path(),
        &case_id,
        "unsupported-report",
        "macos",
        "ready",
        None,
    );
    let case = CaseRepo::new(&case_conn)
        .find_by_id(&case_id)
        .expect("load case")
        .expect("case exists");
    let output = tempfile::TempDir::new().expect("create report output");
    let scope = ExportScopeDto::default();

    let errors = [
        crate::report::generate_json_export_for_case(
            &case_conn,
            &case,
            case_root.path(),
            output.path(),
            &scope,
        )
        .expect_err("JSON export must reject unsupported platform"),
        crate::report::generate_html_report_for_case(
            &case_conn,
            &case,
            case_root.path(),
            output.path(),
            &scope,
        )
        .expect_err("HTML export must reject unsupported platform"),
        crate::report::generate_csv_artifacts_for_case(
            &case_conn,
            &case,
            case_root.path(),
            output.path(),
            &scope,
        )
        .expect_err("CSV export must reject unsupported platform"),
    ];

    for error in errors {
        assert!(matches!(error, ReportError::Unsupported(_)));
        assert!(matches!(error.category(), ErrorCategory::Unsupported));
    }
}

#[test]
fn report_identity_accepts_source_level_timeline_events_without_a_file_reference() {
    let event = TimelineEventDto {
        id: "ds:windows-report:timeline-source-level".to_string(),
        data_source_id: Some("windows-report".to_string()),
        source_object_id: String::new(),
        event_type: "SYSTEM".to_string(),
        ts: "2026-07-11T00:00:00Z".to_string(),
        title: "Source-level event".to_string(),
        description: String::new(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    };

    let source = crate::report::source_identity::timeline_data_source_id(&event)
        .expect("derive source from the global event id");

    assert_eq!(source.0, "windows-report");
}

#[test]
fn report_identity_rejects_cross_source_timeline_references() {
    let event = TimelineEventDto {
        id: "ds:windows-report:timeline-cross-source".to_string(),
        data_source_id: Some("windows-report".to_string()),
        source_object_id: "ds:linux-report:file-1".to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        ts: "2026-07-11T00:00:00Z".to_string(),
        title: "Cross-source event".to_string(),
        description: String::new(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    };

    let error = crate::report::source_identity::timeline_data_source_id(&event)
        .expect_err("reject cross-source identity");

    assert!(error
        .to_string()
        .contains("report record crosses data source boundaries"));
}

fn seed_source_report_records(case_root: &Path, case_id: &CaseId, source_id: &str) {
    let data_source_id = DataSourceId(source_id.to_string());
    let source_conn =
        crate::source_db::open_source_db(case_root, &data_source_id).expect("open source database");
    ArtifactRepo::new(&source_conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId("artifact-shared".to_string()),
                family: "SyntheticArtifact".to_string(),
                title: format!("{source_id} artifact"),
                summary: "source attribution fixture".to_string(),
                source_object_id: Some(FileEntryId(format!("{source_id}-file"))),
                extractor_id: Some("synthetic.report".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(1.0),
                source_attribution: Some("synthetic source database".to_string()),
                created_at: chrono::Utc::now(),
                attrs: BTreeMap::new(),
            }],
            &case_id.0,
            source_id,
        )
        .expect("insert source artifact");
    TimelineRepo::new(&source_conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-shared".to_string()),
                source_object_id: format!("{source_id}-file"),
                event_type: "FILE_MODIFIED".to_string(),
                timestamp: chrono::Utc::now(),
                title: format!("{source_id} timeline event"),
                description: "source attribution fixture".to_string(),
                parser_id: Some("timeline.synthetic".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(1.0),
                source_attribution: Some("modified_at".to_string()),
                attrs: BTreeMap::new(),
            }],
            &case_id.0,
        )
        .expect("insert source timeline event");
}
