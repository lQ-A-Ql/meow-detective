use domain::CaseMeta;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
    report_repo::ReportRepo, timeline_repo::TimelineRepo,
};
use reports::{CsvExporter, HtmlReportExporter, JsonExporter};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use transport::commands::ExportScopeDto;
use transport::dto::{
    AnalysisFileClassificationDto, AnalysisProvenanceDto, AnalysisSystemInfoDto,
    ReportHistoryItemDto, ReportTemplateDto,
};
use uuid::Uuid;

pub fn generate_html_report(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, String> {
    let file_repo = FileRepo::new(conn);
    let tl_repo = TimelineRepo::new(conn);

    let file_count = file_repo.count_all().unwrap_or(0);

    let tl_count = tl_repo.count().map_err(|e| e.to_string())?;

    let files = if scope.file_system_metadata {
        vec![format!("{} files indexed", file_count)]
    } else {
        Vec::new()
    };
    let artifacts = if scope.full_timeline {
        let mut rows = ArtifactRepo::new(conn)
            .list_by_family(None)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|artifact| format_artifact_report_row(&artifact))
            .collect::<Vec<_>>();
        rows.push(format!("{} timeline events", tl_count));
        rows.extend(
            TimelineRepo::new(conn)
                .query(0, 500)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|event| format_timeline_report_row(&event)),
        );
        rows
    } else {
        Vec::new()
    };
    let analysis = current_analysis(conn)?;
    let analysis_rows = report_analysis_rows(conn, &case.id.0, &analysis, scope);

    let file_name = format!("report-{}.html", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    HtmlReportExporter::export_with_analysis(&mut f, case, &files, &artifacts, &analysis_rows)
        .map_err(|e| e.to_string())?;

    persist_report_record(conn, &case.id.0, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}

pub fn generate_csv_artifacts(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, String> {
    let mut stmt = conn.prepare(
        "SELECT artifact_type, title, summary, extractor_id, extractor_version, confidence, source_attribution FROM artifacts ORDER BY created_at DESC LIMIT 1000"
    ).map_err(|e| e.to_string())?;
    let rows_data: Vec<Vec<String>> = stmt
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                row.get::<_, Option<f32>>(5)?
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            ])
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<Vec<String>>, rusqlite::Error>>()
        .map_err(|e| e.to_string())?;
    let mut rows_data = rows_data;
    let analysis = current_analysis(conn)?;
    rows_data.extend(
        report_analysis_rows(conn, case_id, &analysis, scope)
            .into_iter()
            .map(|row| {
                vec![
                    "analysis".to_string(),
                    "provenance".to_string(),
                    row,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]
            }),
    );

    let file_name = format!("artifacts-{}.csv", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    CsvExporter::export_artifacts(
        &mut f,
        &[
            "type",
            "title",
            "summary",
            "extractorId",
            "extractorVersion",
            "confidence",
            "sourceAttribution",
        ],
        &rows_data,
    )
    .map_err(|e| e.to_string())?;

    persist_report_record(conn, case_id, "report-files", &file_name, "completed")?;
    Ok(file_name)
}

pub fn generate_json_export(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, String> {
    let events = if scope.full_timeline {
        TimelineRepo::new(conn)
            .query(0, 500)
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    let artifacts = ArtifactRepo::new(conn)
        .list_by_family(None)
        .map_err(|e| e.to_string())?;
    let analysis = current_analysis(conn)?;
    let warnings = report_warnings(conn, case_id, scope);
    let summary = crate::analysis_service::generate_analysis_summary(
        &analysis.system_info,
        &analysis.classifications,
    );
    let system_info = if scope.registry {
        Some(&analysis.system_info)
    } else {
        None
    };
    let classifications = if scope.file_system_metadata {
        analysis.classifications.as_slice()
    } else {
        &[]
    };
    let json_val = serde_json::json!({
        "timeline_events": events.iter().map(|e| serde_json::json!({
            "id": e.id.0,
            "sourceObjectId": e.source_object_id,
            "type": e.event_type,
            "ts": e.timestamp.to_rfc3339(),
            "title": e.title,
            "description": e.description,
            "parserId": e.parser_id,
            "parserVersion": e.parser_version,
            "confidence": e.confidence,
            "sourceAttribution": e.source_attribution,
        })).collect::<Vec<_>>(),
        "artifacts": artifacts.iter().map(|artifact| serde_json::json!({
            "id": artifact.id.0,
            "artifactType": artifact.family,
            "title": artifact.title,
            "summary": artifact.summary,
            "sourceObjectId": artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
            "extractorId": artifact.extractor_id,
            "extractorVersion": artifact.extractor_version,
            "confidence": artifact.confidence,
            "sourceAttribution": artifact.source_attribution,
            "createdAt": artifact.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "scope": scope,
        "warnings": warnings,
        "analysis": {
            "systemInfo": system_info,
            "classifications": classifications,
            "summary": summary,
        },
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    JsonExporter::export(&mut f, &json_val).map_err(|e| e.to_string())?;

    persist_report_record(conn, case_id, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}

struct ReportAnalysis {
    system_info: AnalysisSystemInfoDto,
    classifications: Vec<AnalysisFileClassificationDto>,
}

fn current_analysis(conn: &Connection) -> Result<ReportAnalysis, String> {
    let system_info =
        crate::analysis_service::extract_system_info_for_case(conn, |file_id, max_bytes| {
            crate::file_service::read_file_header_by_id(conn, file_id, max_bytes)
        });
    let files = crate::analysis_service::collect_file_entries(conn)?;
    let classifications = crate::analysis_service::classify_files_by_magic(
        &files,
        crate::analysis_service::DEFAULT_SAMPLE_SIZE,
        |file_id| {
            crate::file_service::read_file_header_by_id(
                conn,
                file_id,
                crate::analysis_service::MAGIC_HEADER_LIMIT,
            )
        },
    );

    Ok(ReportAnalysis {
        system_info,
        classifications,
    })
}

fn analysis_rows(
    system_info: &AnalysisSystemInfoDto,
    classifications: &[AnalysisFileClassificationDto],
) -> Vec<String> {
    let mut rows = Vec::new();
    rows.push(format!(
        "system_info status={} warnings={}",
        status_str(&system_info.status),
        system_info.warnings.join(" | ")
    ));
    push_optional_analysis_value(
        &mut rows,
        "system_info.computerName",
        &system_info.computer_name,
    );
    push_optional_analysis_value(&mut rows, "system_info.osVersion", &system_info.os_version);
    push_optional_analysis_value(
        &mut rows,
        "system_info.buildNumber",
        &system_info.build_number,
    );
    push_optional_analysis_value(
        &mut rows,
        "system_info.installDate",
        &system_info.install_date,
    );
    push_optional_analysis_value(
        &mut rows,
        "system_info.registeredOwner",
        &system_info.registered_owner,
    );
    push_optional_analysis_value(
        &mut rows,
        "system_info.organization",
        &system_info.organization,
    );
    push_optional_analysis_value(&mut rows, "system_info.productId", &system_info.product_id);
    push_optional_analysis_value(&mut rows, "system_info.timezone", &system_info.timezone);
    rows.extend(
        system_info
            .provenance
            .iter()
            .map(|item| format_provenance("system_info", item)),
    );
    rows.extend(system_info.field_provenance.iter().map(|item| {
        format!(
            "system_info field={} parser={} hive={} key={} valueName={}",
            item.field, item.parser, item.hive_path, item.key_path, item.value_name
        )
    }));
    rows.extend(system_info.boot_history.iter().map(|boot| {
        format!(
            "boot_candidate timestamp={} type={} eventId={} recordId={} source={} note={} provenance={}",
            boot.timestamp,
            boot.boot_type,
            boot.event_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            boot.record_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            boot.source,
            boot.note.as_deref().unwrap_or("-"),
            format_provenance("boot_candidate", &boot.provenance),
        )
    }));

    for classification in classifications {
        rows.push(format!(
            "classification category={} status={} warnings={}",
            classification.category,
            status_str(&classification.status),
            classification.warnings.join(" | ")
        ));
        rows.extend(
            classification
                .provenance
                .iter()
                .map(|item| format_provenance(&classification.category, item)),
        );
    }

    rows
}

fn scoped_analysis_rows(analysis: &ReportAnalysis, scope: &ExportScopeDto) -> Vec<String> {
    let mut rows = Vec::new();
    rows.extend(report_scope_warnings(scope));
    if scope.registry {
        rows.extend(analysis_rows(&analysis.system_info, &[]));
    }
    if scope.file_system_metadata {
        for item in &analysis.classifications {
            rows.push(format!(
                "classification category={} files={} totalSize={} status={} warnings={}",
                item.category,
                item.file_count,
                item.total_size,
                status_str(&item.status),
                item.warnings.join(" | ")
            ));
        }
    }
    rows
}

fn report_analysis_rows(
    conn: &Connection,
    case_id: &str,
    analysis: &ReportAnalysis,
    scope: &ExportScopeDto,
) -> Vec<String> {
    let mut rows = scoped_analysis_rows(analysis, scope);
    rows.extend(evidence_hash_warnings(conn, case_id));
    rows
}

fn report_warnings(conn: &Connection, case_id: &str, scope: &ExportScopeDto) -> Vec<String> {
    let mut warnings = report_scope_warnings(scope);
    warnings.extend(evidence_hash_warnings(conn, case_id));
    warnings
}

fn report_scope_warnings(scope: &ExportScopeDto) -> Vec<String> {
    let mut warnings = Vec::new();
    if scope.raw_file_extraction {
        warnings.push(
            "rawFileExtraction unsupported: raw evidence files were not exported".to_string(),
        );
    }
    warnings
}

fn evidence_hash_warnings(conn: &Connection, case_id: &str) -> Vec<String> {
    let repo = DataSourceRepo::new(conn);
    let sources = repo
        .find_by_case(&domain::CaseId(case_id.to_string()))
        .unwrap_or_default();
    let mut pending = 0;
    let mut failed = 0;
    let mut unavailable = 0;
    let mut unknown = 0;

    for source in sources {
        match source.provenance.hash_status {
            domain::DataSourceHashStatus::Pending => pending += 1,
            domain::DataSourceHashStatus::Failed => failed += 1,
            domain::DataSourceHashStatus::Unavailable => unavailable += 1,
            domain::DataSourceHashStatus::Unknown => unknown += 1,
            domain::DataSourceHashStatus::Hashed => {}
        }
    }

    evidence_hash_warning_messages(pending, failed, unavailable, unknown)
}

fn evidence_hash_warning_messages(
    pending: usize,
    failed: usize,
    unavailable: usize,
    unknown: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if pending > 0 {
        warnings.push(format!(
            "evidenceHash pending: {pending} data source(s) still require background hash verification"
        ));
    }
    if failed > 0 {
        warnings.push(format!(
            "evidenceHash failed: {failed} data source(s) require manual verification"
        ));
    }
    if unavailable > 0 {
        warnings.push(format!(
            "evidenceHash unavailable: {unavailable} data source(s) cannot provide source hash verification"
        ));
    }
    if unknown > 0 {
        warnings.push(format!(
            "evidenceHash deferred: {unknown} data source(s) have unknown hash verification status"
        ));
    }
    warnings
}

fn format_artifact_report_row(artifact: &domain::Artifact) -> String {
    format!(
        "artifact type={} title={} summary={} extractor={} extractorVersion={} confidence={} sourceAttribution={}",
        artifact.family,
        artifact.title,
        artifact.summary,
        optional_str(&artifact.extractor_id),
        optional_str(&artifact.extractor_version),
        optional_f32(artifact.confidence),
        optional_str(&artifact.source_attribution)
    )
}

fn format_timeline_report_row(event: &domain::TimelineEvent) -> String {
    format!(
        "timeline eventType={} timestamp={} title={} parser={} parserVersion={} confidence={} sourceAttribution={}",
        event.event_type,
        event.timestamp.to_rfc3339(),
        event.title,
        optional_str(&event.parser_id),
        optional_str(&event.parser_version),
        optional_f32(event.confidence),
        optional_str(&event.source_attribution)
    )
}

fn optional_str(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("unknown")
}

fn optional_f32(value: Option<f32>) -> String {
    value
        .map(|confidence| confidence.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn push_optional_analysis_value(rows: &mut Vec<String>, field: &str, value: &Option<String>) {
    if let Some(value) = value {
        rows.push(format!("{field}={value}"));
    }
}

fn format_provenance(scope: &str, item: &AnalysisProvenanceDto) -> String {
    format!(
        "{} parser={} status={} dataSource={} artifact={} parsedAt={} warnings={}",
        scope,
        item.parser,
        status_str(&item.status),
        item.data_source_id,
        item.artifact_path,
        item.parsed_at,
        item.warnings.join(" | ")
    )
}

fn status_str(status: &transport::dto::AnalysisParseStatusDto) -> &'static str {
    match status {
        transport::dto::AnalysisParseStatusDto::Parsed => "parsed",
        transport::dto::AnalysisParseStatusDto::Partial => "partial",
        transport::dto::AnalysisParseStatusDto::NotParsed => "notParsed",
        transport::dto::AnalysisParseStatusDto::Unavailable => "unavailable",
        transport::dto::AnalysisParseStatusDto::CandidateFound => "candidateFound",
        transport::dto::AnalysisParseStatusDto::NotFound => "notFound",
        transport::dto::AnalysisParseStatusDto::Failed => "failed",
    }
}

pub fn get_report_templates() -> Vec<ReportTemplateDto> {
    vec![
        ReportTemplateDto {
            id: "report-summary".into(),
            name: "案件摘要报告".into(),
            description: "输出案件基础信息、关键时间线与工件摘要。".into(),
        },
        ReportTemplateDto {
            id: "report-files".into(),
            name: "文件活动报告".into(),
            description: "输出可疑文件、哈希与访问活动。".into(),
        },
    ]
}

pub fn get_report_history(conn: &Connection, case_id: &str) -> Vec<ReportHistoryItemDto> {
    let repo = ReportRepo::new(conn);
    let records = match repo.list_by_case(case_id) {
        Ok(records) => records,
        Err(_) => return Vec::new(),
    };
    records
        .into_iter()
        .map(|r| ReportHistoryItemDto {
            id: r.id,
            file_name: r.file_name,
            created_by: r.created_by,
            created_at: r.created_at,
            status: r.status,
            progress: r.progress,
        })
        .collect()
}

fn persist_report_record(
    conn: &Connection,
    case_id: &str,
    template_id: &str,
    file_name: &str,
    status: &str,
) -> Result<(), String> {
    let repo = ReportRepo::new(conn);
    let record = persistence_sqlite::repositories::report_repo::ReportRecord {
        id: Uuid::new_v4().to_string(),
        case_id: case_id.to_string(),
        template_id: template_id.to_string(),
        file_name: file_name.to_string(),
        created_by: String::new(),
        status: status.to_string(),
        progress: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    repo.insert(&record).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind,
        EntryType, FileEntry, FileEntryId, TimelineEvent, TimelineEventId,
    };
    use persistence_sqlite::repositories::{
        case_repo::CaseRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
        timeline_repo::TimelineRepo,
    };
    use persistence_sqlite::{open_in_memory, runner};
    use tempfile::TempDir;
    use transport::dto::{
        AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisParseStatusDto,
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
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])
            .unwrap();
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
        persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(conn)
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

    #[test]
    fn json_export_includes_analysis_provenance_without_fake_facts() {
        let (conn, tmp, case, ds_id) = setup_report_case();
        insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");

        let file_name =
            generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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
            generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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

        assert!(error.contains("reports"));
    }

    #[test]
    fn json_export_scope_gates_registry_timeline_and_warns_raw_unsupported() {
        let (conn, tmp, case, ds_id) = setup_report_case();
        insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");
        insert_timeline_event(&conn, &case.id.0);
        let scope = ExportScopeDto {
            file_system_metadata: true,
            registry: false,
            full_timeline: false,
            raw_file_extraction: true,
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
                .contains("rawFileExtraction unsupported")));
        assert!(!tmp.path().join("raw").exists());
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
            generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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
            generate_csv_artifacts(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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
            generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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
            generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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
            generate_json_export(&conn, &case.id.0, tmp.path(), &ExportScopeDto::default())
                .unwrap();
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

        let rows = analysis_rows(&system_info, &[]);
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
}
