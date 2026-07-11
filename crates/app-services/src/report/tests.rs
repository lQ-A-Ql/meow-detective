use super::analysis_rows::system_info_rows;
use super::json::sanitize_bundle_component;
use super::*;
use domain::{
    Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, EntryType,
    FileEntry, FileEntryId, TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo as TestArtifactRepo, case_repo::CaseRepo,
    datasource_repo::DataSourceRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use persistence_sqlite::{open_in_memory, runner};
use std::collections::BTreeMap;
use tempfile::TempDir;
use transport::commands::ExportScopeDto;

use transport::dto::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisParseStatusDto,
    AnalysisProvenanceDto, AnalysisSystemInfoDto,
};

fn setup_report_case() -> (rusqlite::Connection, TempDir, CaseMeta, DataSourceId) {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    let case = CaseMeta {
        id: CaseId("case-report".to_string()),
        name: "<Report Case>".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(&conn).create(&case).unwrap();

    let tmp = TempDir::new().unwrap();
    let ds_id = DataSourceId("ds-report".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &case.id,
            &DataSource {
                id: ds_id.clone(),
                name: "logical".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: tmp.path().to_path_buf(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    (conn, tmp, case, ds_id)
}

fn insert_file(conn: &rusqlite::Connection, ds_id: &DataSourceId, id: &str, path: &str) {
    insert_file_with_hash(conn, ds_id, id, path, None);
}

fn insert_file_with_hash(
    conn: &rusqlite::Connection,
    ds_id: &DataSourceId,
    id: &str,
    path: &str,
    hash_sha256: Option<&str>,
) {
    let source_root: String = conn
        .query_row(
            "SELECT source_path FROM data_sources WHERE id = ?1",
            rusqlite::params![ds_id.0],
            |row| row.get(0),
        )
        .unwrap();
    let disk_relative_path = test_disk_relative_path(path);
    let disk_path = std::path::PathBuf::from(source_root).join(disk_relative_path);
    if let Some(parent) = disk_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&disk_path, format!("fixture:{id}")).unwrap();
    FileRepo::new(conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: ds_id.clone(),
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            entry_type: EntryType::File,
            size: Some(4),
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: hash_sha256.map(|value| value.to_string()),
        }])
        .unwrap();
}

fn test_disk_relative_path(path: &str) -> std::path::PathBuf {
    path.split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .map(sanitize_bundle_component)
        .collect()
}

fn insert_timeline_event(conn: &rusqlite::Connection, case_id: &str) {
    TimelineRepo::new(conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-1".to_string()),
                source_object_id: "file-1".to_string(),
                event_type: "file_modified".to_string(),
                timestamp: chrono::Utc::now(),
                title: "Timeline Scope Event".to_string(),
                description: "scope fixture".to_string(),
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: std::collections::BTreeMap::new(),
            }],
            case_id,
        )
        .unwrap();
}

fn insert_timeline_event_with_provenance(conn: &rusqlite::Connection, case_id: &str) {
    TimelineRepo::new(conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-provenance".to_string()),
                source_object_id: "file-1".to_string(),
                event_type: "file_modified".to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-04T12:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                title: "Timeline Provenance Event".to_string(),
                description: "timeline provenance fixture".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.2.3".to_string()),
                confidence: Some(0.82),
                source_attribution: Some("modified_at".to_string()),
                attrs: std::collections::BTreeMap::new(),
            }],
            case_id,
        )
        .unwrap();
}

fn insert_artifact_with_provenance(
    conn: &rusqlite::Connection,
    case_id: &str,
    ds_id: &DataSourceId,
) {
    TestArtifactRepo::new(conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId("artifact-provenance".to_string()),
                family: "prefetch".to_string(),
                title: "CMD.EXE-12345678.pf".to_string(),
                summary: "Prefetch execution evidence".to_string(),
                source_object_id: Some(FileEntryId("file-1".to_string())),
                extractor_id: Some("prefetch".to_string()),
                extractor_version: Some("1.2.3".to_string()),
                confidence: Some(0.93),
                source_attribution: Some("Windows/Prefetch/CMD.EXE-12345678.pf".to_string()),
                created_at: chrono::DateTime::parse_from_rfc3339("2026-06-04T10:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                attrs: std::collections::BTreeMap::new(),
            }],
            case_id,
            &ds_id.0,
        )
        .unwrap();
}

fn insert_artifact_and_timeline_for_correlation(
    conn: &rusqlite::Connection,
    case_id: &str,
    ds_id: &DataSourceId,
) {
    insert_file(conn, ds_id, "file-1", "C:/Windows/System32/cmd.exe");
    TestArtifactRepo::new(conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId("artifact-correlation".to_string()),
                family: "LNK".to_string(),
                title: "cmd.lnk".to_string(),
                summary: "target -> cmd.exe".to_string(),
                source_object_id: Some(FileEntryId("file-1".to_string())),
                extractor_id: Some("lnk".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.91),
                source_attribution: Some("Users/Admin/Desktop/cmd.lnk".to_string()),
                created_at: chrono::Utc::now(),
                attrs: std::collections::BTreeMap::new(),
            }],
            case_id,
            &ds_id.0,
        )
        .unwrap();
    TimelineRepo::new(conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-correlation".to_string()),
                source_object_id: "file-1".to_string(),
                event_type: "FILE_MODIFIED".to_string(),
                timestamp: chrono::Utc::now(),
                title: "cmd.exe modified".to_string(),
                description: "timeline correlation fixture".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.82),
                source_attribution: Some("modified_at".to_string()),
                attrs: std::collections::BTreeMap::new(),
            }],
            case_id,
        )
        .unwrap();
}

#[test]
fn json_export_includes_analysis_provenance_without_fake_facts() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");

    let file_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

    assert!(json.contains("\"analysis\""));
    assert!(json.contains("\"provenance\""));
    assert!(json.contains("registry.system"));
    assert!(!json.contains("FORENSICS-PC"));
    assert!(!json.contains("Windows 10"));
}

#[test]
fn html_report_escapes_analysis_provenance() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file(
        &conn,
        &ds_id,
        "evil",
        "Windows/System32/config/<script>alert(1)</script>",
    );

    let file_name =
        generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap();
    let html = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

    assert!(html.contains("Analysis Provenance"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
}

#[test]
fn csv_report_keeps_formula_sanitization_for_analysis_rows() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file(&conn, &ds_id, "formula", "=SUM(A1:A2)");
    conn.execute(
        "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title, summary, attrs, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "artifact-formula",
            "case-report",
            ds_id.0,
            "lnk",
            "=SUM(A1:A2)",
            "formula title fixture",
            "{}",
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .unwrap();

    let file_name =
        generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let csv = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

    assert!(csv.contains("\"analysis\""));
    assert!(csv.contains("provenance"));
    assert!(csv.contains("\"\t=SUM(A1:A2)\""));
}

#[test]
fn report_exports_persist_history_for_active_case_only() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");

    generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap();
    generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();

    let history = get_report_history(&conn, &case.id.0);
    assert_eq!(history.len(), 3);
    assert!(history.iter().any(|item| item.file_name.ends_with(".html")));
    assert!(history.iter().any(|item| item.file_name.ends_with(".csv")));
    assert!(history.iter().any(|item| item.file_name.ends_with(".json")));
    assert!(get_report_history(&conn, "case-other").is_empty());
}

#[test]
fn report_export_returns_error_when_history_insert_fails() {
    let (conn, tmp, case, _ds_id) = setup_report_case();
    conn.execute_batch("DROP TABLE reports").unwrap();

    let error =
        generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap_err();

    assert!(error.to_string().contains("reports"));
}

#[test]
fn json_export_scope_gates_registry_timeline_and_exports_raw_bundle() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file_with_hash(
        &conn,
        &ds_id,
        "system",
        "Windows/System32/config/SYSTEM",
        Some("existinghash"),
    );
    insert_timeline_event(&conn, &case.id.0);
    let scope = ExportScopeDto {
        file_system_metadata: true,
        registry: false,
        full_timeline: false,
        raw_file_extraction: true,
        overwrite: false,
    };

    let file_name = generate_json_export(&conn, &case.id.0, tmp.path(), &scope).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(file_name)).unwrap())
            .unwrap();

    assert!(json["timeline_events"].as_array().unwrap().is_empty());
    assert!(json["analysis"]["systemInfo"].is_null());
    assert!(!json["analysis"]["classifications"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap()
            .contains("rawFileExtraction exported")));
    let bundle_dir = tmp
        .path()
        .join(json["rawExport"]["bundleDirectory"].as_str().unwrap());
    assert!(bundle_dir.exists());
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(bundle_dir.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["exportedCount"].as_u64(), Some(1));
    assert_eq!(manifest["files"][0]["fileId"], "system");
    assert_eq!(
        manifest["files"][0]["relativeSourcePath"],
        "Windows/System32/config/SYSTEM"
    );
    let hashes = std::fs::read_to_string(bundle_dir.join("SHA256SUMS.txt")).unwrap();
    assert!(hashes.contains("files/ds-report/system-SYSTEM"));
}

#[test]
fn json_export_scope_can_hide_file_classifications() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");
    let scope = ExportScopeDto {
        file_system_metadata: false,
        registry: true,
        full_timeline: true,
        raw_file_extraction: false,
        overwrite: false,
    };

    let file_name = generate_json_export(&conn, &case.id.0, tmp.path(), &scope).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(file_name)).unwrap())
            .unwrap();

    assert!(json["analysis"]["classifications"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!json["analysis"]["systemInfo"].is_null());
}

#[test]
fn report_exports_include_artifact_provenance() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_artifact_with_provenance(&conn, &case.id.0, &ds_id);

    let json_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(json_name)).unwrap())
            .unwrap();
    let artifact = &json["artifacts"][0];
    assert_eq!(artifact["extractorId"], "prefetch");
    assert_eq!(artifact["extractorVersion"], "1.2.3");
    assert!((artifact["confidence"].as_f64().unwrap() - 0.93).abs() < 0.000001);
    assert_eq!(
        artifact["sourceAttribution"],
        "Windows/Prefetch/CMD.EXE-12345678.pf"
    );

    let csv_name =
        generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let csv = std::fs::read_to_string(tmp.path().join(csv_name)).unwrap();
    assert!(csv.contains("extractorId,extractorVersion,confidence,sourceAttribution"));
    assert!(csv.contains("\"prefetch\",\"1.2.3\",\"0.93\""));
    assert!(csv.contains("Windows/Prefetch/CMD.EXE-12345678.pf"));
}

#[test]
fn report_exports_include_timeline_provenance() {
    let (conn, tmp, case, _ds_id) = setup_report_case();
    insert_timeline_event_with_provenance(&conn, &case.id.0);

    let json_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(json_name)).unwrap())
            .unwrap();
    let event = &json["timeline_events"][0];
    assert_eq!(event["parserId"], "timeline.macb");
    assert_eq!(event["parserVersion"], "1.2.3");
    assert!((event["confidence"].as_f64().unwrap() - 0.82).abs() < 0.000001);
    assert_eq!(event["sourceAttribution"], "modified_at");

    let html_name =
        generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap();
    let html = std::fs::read_to_string(tmp.path().join(html_name)).unwrap();
    assert!(html.contains("timeline.macb"));
    assert!(html.contains("parserVersion=1.2.3"));
    assert!(html.contains("confidence=0.82"));
    assert!(html.contains("sourceAttribution=modified_at"));
}

#[test]
fn report_exports_include_correlation_section() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_artifact_and_timeline_for_correlation(&conn, &case.id.0, &ds_id);

    let json_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(json_name)).unwrap())
            .unwrap();

    assert!(json["correlation"]["leadCount"].as_u64().unwrap() >= 1);
    assert_eq!(json["governance"]["releaseScorecard"]["grade"], "C");
    assert!(json["governance"]["factSources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["factFile"] == "testdata/governance/v2-release-policy.json"));
    assert!(json["governance"]["factSources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["factFile"] == "testdata/governance/v2-known-limitations.json"));
    assert!(json["governance"]["factSources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["factFile"] == "testdata/governance/v2-runtime-results.json" }));
    assert!(json["governance"]["knownLimitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["category"] == "Browser" && item["status"] == "unsupported"));
    assert!(json["governance"]["runtimeResults"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["checkId"] == "docs-drift"));
    assert!(json["governance"]["runtimeResults"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["subChecks"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false)));
    assert!(json["governance"]["releaseGates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["gateId"] == "security-baseline"));
    assert!(
        json["governance"]["runtimeSignals"]["correlationFamilyCoverage"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["family"] == "LNK")
    );
    assert!(json["governance"]["benchmark"]["requiredChecks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["scenario"] == "file_tree_expand"));
    assert_eq!(json["governance"]["benchmark"]["missingRequiredCount"], 0);
    assert_eq!(json["correlation"]["leads"][0]["primaryFileId"], "file-1");
    assert!(json["correlation"]["leads"][0]["summary"]
        .as_str()
        .unwrap()
        .contains("source object"));

    let html_name =
        generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap();
    let html = std::fs::read_to_string(tmp.path().join(html_name)).unwrap();
    assert!(html.contains("Governance Snapshot"));
    assert!(html.contains("governance summary generatedAt="));
    assert!(html.contains("governance runtimeCheck=docs-drift"));
    assert!(html.contains("governance runtimeSubcheck=readme-fact-sync parent=docs-drift"));
    assert!(html.contains(
        "governance factSource area=knownLimitations factFile=testdata/governance/v2-known-limitations.json"
    ));
    assert!(html.contains(
        "governance knownLimitation category=Recycle Bin item=全损坏恢复场景 status=notGuaranteed"
    ));
    assert!(html.contains("governance benchmarkSummary baselineVersion="));
    assert!(html.contains(
        "governance benchmarkCheck datasetLevel=medium scenario=file_tree_expand status=covered"
    ));
    assert!(html.contains("governance gate=security-baseline"));
    assert!(html.contains("governance correlationFamily family=LNK"));
    assert!(html.contains("Correlation Leads"));
    assert!(html.contains("primaryFileId=file-1"));
    assert!(html.contains("confidence=direct"));

    let csv_name =
        generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let csv = std::fs::read_to_string(tmp.path().join(csv_name)).unwrap();
    assert!(csv.contains("\"governance\""));
    assert!(csv.contains("governance summary generatedAt="));
    assert!(csv.contains("governance runtimeCheck=docs-drift"));
    assert!(csv.contains("governance runtimeSubcheck=readme-fact-sync parent=docs-drift"));
    assert!(csv.contains(
        "governance factSource area=knownLimitations factFile=testdata/governance/v2-known-limitations.json"
    ));
    assert!(csv.contains(
        "governance knownLimitation category=Recycle Bin item=全损坏恢复场景 status=notGuaranteed"
    ));
    assert!(csv.contains("governance benchmarkSummary baselineVersion="));
    assert!(csv.contains(
        "governance benchmarkCheck datasetLevel=medium scenario=file_tree_expand status=covered"
    ));
    assert!(csv.contains("governance gate=security-baseline"));
    assert!(csv.contains("governance correlationFamily family=LNK"));
    assert!(csv.contains("correlation summary leads="));
    assert!(csv.contains("cmd.lnk"));
}

#[test]
fn report_exports_tolerate_legacy_missing_provenance() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    conn.execute(
        "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title, summary, attrs, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "artifact-legacy",
            case.id.0,
            ds_id.0,
            "legacy",
            "Legacy Artifact",
            "legacy summary",
            "{}",
            "2026-06-04T09:00:00Z",
        ],
    )
    .unwrap();
    insert_timeline_event(&conn, &case.id.0);

    let json_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(json_name)).unwrap())
            .unwrap();
    assert!(json["artifacts"][0]["extractorId"].is_null());
    assert!(json["artifacts"][0]["confidence"].is_null());
    assert!(json["timeline_events"][0]["parserId"].is_null());
    assert!(json["timeline_events"][0]["sourceAttribution"].is_null());

    let html_name =
        generate_html_report(&conn, &case, tmp.path(), &ExportScopeDto::default()).unwrap();
    let html = std::fs::read_to_string(tmp.path().join(html_name)).unwrap();
    assert!(html.contains("extractor=unknown"));
    assert!(html.contains("parser=unknown"));
    assert!(html.contains("confidence=unknown"));
}

#[test]
fn report_scope_warning_reports_empty_raw_export_bundle() {
    let scope = ExportScopeDto {
        file_system_metadata: true,
        registry: true,
        full_timeline: true,
        raw_file_extraction: true,
        overwrite: false,
    };

    let warnings = report_scope_warnings(&scope, None);

    assert!(warnings.iter().any(|warning| warning
        .contains("rawFileExtraction requested but no eligible files were exported")));
}

#[test]
fn json_export_warns_when_evidence_hash_is_pending_or_unavailable() {
    let (conn, tmp, case, _ds_id) = setup_report_case();
    let pending = DataSourceId("ds-pending".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &case.id,
            &DataSource {
                id: pending,
                name: "pending-source".to_string(),
                kind: DataSourceKind::Raw,
                source_path: tmp.path().join("pending.raw"),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance {
                    source_hash_sha256: None,
                    hash_status: domain::DataSourceHashStatus::Pending,
                    canonical_source_path: None,
                    evidence_size: Some(4096),
                    reader_kind: Some("raw".to_string()),
                    provenance_status: domain::DataSourceProvenanceStatus::Recorded,
                    warnings: Vec::new(),
                },
            },
        )
        .unwrap();
    let unavailable = DataSourceId("ds-unavailable".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &case.id,
            &DataSource {
                id: unavailable,
                name: "unavailable-source".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: tmp.path().join("logical"),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance {
                    source_hash_sha256: None,
                    hash_status: domain::DataSourceHashStatus::Unavailable,
                    canonical_source_path: None,
                    evidence_size: None,
                    reader_kind: Some("logical_directory".to_string()),
                    provenance_status: domain::DataSourceProvenanceStatus::Recorded,
                    warnings: Vec::new(),
                },
            },
        )
        .unwrap();

    let json_name =
        generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(json_name)).unwrap())
            .unwrap();
    let warnings = json["warnings"].as_array().unwrap();

    assert!(warnings
        .iter()
        .any(|warning| warning.as_str().unwrap().contains("evidenceHash pending")));
    assert!(warnings.iter().any(|warning| warning
        .as_str()
        .unwrap()
        .contains("evidenceHash unavailable")));
    assert!(!json.to_string().contains("pending.raw"));
}

#[test]
fn analysis_rows_include_field_and_boot_provenance() {
    let parsed_at = "2026-06-01T10:00:00Z".to_string();
    let system_info = AnalysisSystemInfoDto {
        computer_name: Some("BETA-LAB".to_string()),
        os_version: Some("Windows Evidence Edition 24H2".to_string()),
        build_number: Some("26000".to_string()),
        install_date: None,
        registered_owner: None,
        organization: None,
        product_id: None,
        network_adapters: Vec::new(),
        boot_history: vec![AnalysisBootRecordDto {
            timestamp: "2026-06-01T08:15:00Z".to_string(),
            boot_type: "eventLogStarted".to_string(),
            source: "Windows/System32/winevt/Logs/System.evtx".to_string(),
            event_id: Some(6005),
            record_id: Some(42),
            note: Some("EventLog 6005 candidate".to_string()),
            details: BTreeMap::new(),
            provenance: AnalysisProvenanceDto {
                data_source_id: "ds-report".to_string(),
                artifact_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
                parser: "evtx.boot_shutdown".to_string(),
                parsed_at: parsed_at.clone(),
                status: AnalysisParseStatusDto::Parsed,
                warnings: Vec::new(),
            },
        }],
        timezone: Some("China Standard Time".to_string()),
        language: None,
        status: AnalysisParseStatusDto::Parsed,
        warnings: Vec::new(),
        provenance: vec![AnalysisProvenanceDto {
            data_source_id: "ds-report".to_string(),
            artifact_path: "Windows/System32/config/SYSTEM".to_string(),
            parser: "registry.system".to_string(),
            parsed_at,
            status: AnalysisParseStatusDto::Parsed,
            warnings: Vec::new(),
        }],
        field_provenance: vec![AnalysisFieldProvenanceDto {
            field: "computerName".to_string(),
            value_name: "ComputerName".to_string(),
            key_path: "ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
            hive_path: "Windows/System32/config/SYSTEM".to_string(),
            parser: "registry.system".to_string(),
        }],
    };

    let rows = system_info_rows(&system_info);
    let joined = rows.join("\n");

    assert!(joined.contains("system_info.computerName=BETA-LAB"));
    assert!(joined.contains("system_info.osVersion=Windows Evidence Edition 24H2"));
    assert!(joined.contains("field=computerName"));
    assert!(joined.contains("key=ControlSet001\\Control\\ComputerName\\ComputerName"));
    assert!(joined.contains("boot_candidate timestamp=2026-06-01T08:15:00Z"));
    assert!(joined.contains("eventId=6005"));
    assert!(joined.contains("recordId=42"));
    assert!(joined.contains("evtx.boot_shutdown"));
    assert!(!joined.contains("FORENSICS-PC"));
}

#[test]
fn csv_correlation_export_includes_all_columns() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_artifact_and_timeline_for_correlation(&conn, &case.id.0, &ds_id);

    let file_name =
        generate_csv_correlation(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
            .unwrap();
    let csv = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

    // Verify header row contains all 9 columns
    assert!(csv.contains("lead_id,title,confidence,families,primary_file_path,supporting_node_count,match_signals_count,provenance_sources,caveats"));

    // Verify at least one data row with real correlation data
    assert!(csv.contains("\"cmd.exe 形成关联线索\""));
    assert!(csv.contains("\"direct\""));
    assert!(csv.contains("LNK"));
    assert!(csv.contains("\"file-1\""));

    // Verify history record was persisted (dedicated correlation CSV file)
    let history = get_report_history(&conn, &case.id.0);
    assert!(
        history
            .iter()
            .any(|item| item.file_name.starts_with("correlation-")
                && item.file_name.ends_with(".csv"))
    );
}

#[test]
fn csv_correlation_export_persists_history() {
    let (conn, tmp, case, ds_id) = setup_report_case();
    insert_artifact_and_timeline_for_correlation(&conn, &case.id.0, &ds_id);

    let file_name =
        generate_csv_correlation(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
            .unwrap();

    let csv_path = tmp.path().join(&file_name);
    assert!(csv_path.exists());

    let csv = std::fs::read_to_string(&csv_path).unwrap();
    // File name starts with "correlation-" but CSV content has the structured header
    assert!(csv.contains("lead_id"));

    let history = get_report_history(&conn, &case.id.0);
    let correlation_items: Vec<_> = history
        .iter()
        .filter(|item| item.file_name.starts_with("correlation-"))
        .collect();
    assert_eq!(correlation_items.len(), 1);
    assert_eq!(correlation_items[0].file_name, file_name);
    assert_eq!(correlation_items[0].status, "completed");
}

#[test]
fn csv_correlation_export_scope_gates_empty_when_no_scope() {
    let (conn, tmp, case, _ds_id) = setup_report_case();
    // No artifacts or timeline — correlation snapshot should be empty
    let scope = ExportScopeDto {
        file_system_metadata: false,
        registry: false,
        full_timeline: false,
        raw_file_extraction: false,
        overwrite: false,
    };

    let file_name = generate_csv_correlation(&conn, &case.id.0, tmp.path(), &scope).unwrap();
    let csv = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

    // Header should exist even with empty data (no rows)
    assert!(csv.contains("lead_id,title,confidence,families,primary_file_path,supporting_node_count,match_signals_count,provenance_sources,caveats"));
}
