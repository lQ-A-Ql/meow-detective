use domain::CaseMeta;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
    report_repo::ReportRepo, timeline_repo::TimelineRepo,
};
use reports::{CsvExporter, HtmlReportExporter, JsonExporter};
use rusqlite::Connection;
use sha2::Digest;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    write_report_atomically(&path, scope.overwrite, |file| {
        HtmlReportExporter::export_with_analysis(file, case, &files, &artifacts, &analysis_rows)
            .map_err(|e| e.to_string())
    })?;

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
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    write_report_atomically(&path, scope.overwrite, |file| {
        CsvExporter::export_artifacts(
            file,
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
        .map_err(|e| e.to_string())
    })?;

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
        "warnings": serde_json::Value::Array(Vec::new()),
        "analysis": {
            "systemInfo": system_info,
            "classifications": classifications,
            "summary": summary,
        },
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    let raw_bundle = if scope.raw_file_extraction {
        Some(export_raw_file_bundle(
            conn,
            output_dir,
            case_id,
            &file_name,
            scope.overwrite,
        )?)
    } else {
        None
    };
    let warnings = report_warnings(conn, case_id, scope, raw_bundle.as_ref());
    write_report_atomically(&path, scope.overwrite, |file| {
        let mut payload = json_val;
        if let Some(bundle) = &raw_bundle {
            payload["rawExport"] = serde_json::json!({
                "bundleDirectory": bundle.bundle_dir_name,
                "manifestFile": bundle.manifest_file_name,
                "hashesFile": bundle.hashes_file_name,
                "exportedCount": bundle.exported_count,
            });
        }
        payload["warnings"] = serde_json::to_value(&warnings).map_err(|e| e.to_string())?;
        JsonExporter::export(file, &payload).map_err(|e| e.to_string())
    })?;

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
    rows.extend(report_scope_warnings(scope, None));
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

fn report_warnings(
    conn: &Connection,
    case_id: &str,
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = report_scope_warnings(scope, raw_bundle);
    warnings.extend(evidence_hash_warnings(conn, case_id));
    warnings
}

fn report_scope_warnings(
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if scope.raw_file_extraction {
        match raw_bundle {
            Some(bundle) => warnings.push(format!(
                "rawFileExtraction exported: {} file(s) copied into {}",
                bundle.exported_count, bundle.bundle_dir_name
            )),
            None => warnings.push(
                "rawFileExtraction requested but no eligible files were exported".to_string(),
            ),
        }
    }
    warnings
}

struct RawExportBundle {
    bundle_dir_name: String,
    manifest_file_name: String,
    hashes_file_name: String,
    exported_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifestEntry {
    file_id: String,
    data_source_id: String,
    relative_source_path: String,
    exported_relative_path: String,
    size: Option<u64>,
    sha256: Option<String>,
    deleted: bool,
    hidden: bool,
    system: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawExportManifest {
    case_id: String,
    generated_from_report: String,
    exported_count: usize,
    files: Vec<RawExportManifestEntry>,
}

fn export_raw_file_bundle(
    conn: &Connection,
    output_dir: &Path,
    case_id: &str,
    report_file_name: &str,
    overwrite: bool,
) -> Result<RawExportBundle, String> {
    let bundle_dir_name = bundle_dir_name_from_report(report_file_name);
    let bundle_dir = output_dir.join(&bundle_dir_name);
    prepare_bundle_directory(&bundle_dir, overwrite)?;

    let entries = collect_exportable_file_entries(conn)?;
    let export_root = bundle_dir.join("files");
    fs::create_dir_all(&export_root).map_err(|e| e.to_string())?;

    let mut manifest_entries = Vec::new();
    let mut hash_lines = Vec::new();

    for entry in entries {
        let mut reader = match crate::file_service::open_file_content_by_id(conn, &entry.id) {
            Ok(reader) => reader,
            Err(_) => continue,
        };

        let safe_name = sanitize_bundle_component(&entry.name);
        let export_rel = PathBuf::from(entry.data_source_id.0.clone())
            .join(format!("{}-{}", entry.id.0, safe_name));
        let export_path = export_root.join(&export_rel);
        if let Some(parent) = export_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&export_path)
            .map_err(|e| e.to_string())?;

        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut total_bytes = 0u64;
        loop {
            let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|e| e.to_string())?;
            hasher.update(&buffer[..read]);
            total_bytes = total_bytes.saturating_add(read as u64);
        }
        output.flush().map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;

        let sha256 = format!("{:x}", hasher.finalize());
        hash_lines.push(format!(
            "{}  {}",
            sha256,
            normalize_manifest_path(&PathBuf::from("files").join(&export_rel))
        ));
        manifest_entries.push(RawExportManifestEntry {
            file_id: entry.id.0.clone(),
            data_source_id: entry.data_source_id.0.clone(),
            relative_source_path: entry.path.clone(),
            exported_relative_path: normalize_manifest_path(
                &PathBuf::from("files").join(&export_rel),
            ),
            size: entry.size.or(Some(total_bytes)),
            sha256: Some(sha256),
            deleted: entry.deleted,
            hidden: entry.hidden,
            system: entry.system,
        });
    }

    let manifest = RawExportManifest {
        case_id: case_id.to_string(),
        generated_from_report: report_file_name.to_string(),
        exported_count: manifest_entries.len(),
        files: manifest_entries,
    };
    let manifest_file_name = "manifest.json".to_string();
    let hashes_file_name = "SHA256SUMS.txt".to_string();
    fs::write(
        bundle_dir.join(&manifest_file_name),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(bundle_dir.join(&hashes_file_name), hash_lines.join("\n"))
        .map_err(|e| e.to_string())?;

    Ok(RawExportBundle {
        bundle_dir_name,
        manifest_file_name,
        hashes_file_name,
        exported_count: manifest.exported_count,
    })
}

fn collect_exportable_file_entries(conn: &Connection) -> Result<Vec<domain::FileEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE entry_type = 'file'
             ORDER BY data_source_id ASC, path ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let entry_type: String = row.get(5)?;
            Ok(domain::FileEntry {
                id: domain::FileEntryId(row.get::<_, String>(0)?),
                parent_id: row.get::<_, Option<String>>(1)?.map(domain::FileEntryId),
                data_source_id: domain::DataSourceId(row.get::<_, String>(2)?),
                path: row.get(3)?,
                name: row.get(4)?,
                entry_type: if entry_type.eq_ignore_ascii_case("directory") {
                    domain::EntryType::Directory
                } else {
                    domain::EntryType::File
                },
                size: row.get(6)?,
                ext: row.get(7)?,
                deleted: row.get::<_, i32>(8)? != 0,
                hidden: row.get::<_, i32>(9)? != 0,
                system: row.get::<_, i32>(10)? != 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| e.to_string())?);
    }
    Ok(entries)
}

fn bundle_dir_name_from_report(report_file_name: &str) -> String {
    let stem = Path::new(report_file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    format!("{stem}-bundle")
}

fn prepare_bundle_directory(bundle_dir: &Path, overwrite: bool) -> Result<(), String> {
    if bundle_dir.exists() {
        if !overwrite {
            return Err(format!(
                "raw export bundle already exists: {} (set overwrite=true to replace it)",
                bundle_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("bundle")
            ));
        }
        fs::remove_dir_all(bundle_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())
}

fn sanitize_bundle_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if sanitized.trim().is_empty() {
        "file".to_string()
    } else {
        sanitized
    }
}

fn normalize_manifest_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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

fn prepare_report_output(
    output_dir: &Path,
    file_name: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    let path = output_dir.join(file_name);
    if path.exists() && !overwrite {
        return Err(format!(
            "report output already exists: {} (set overwrite=true to replace it)",
            file_name
        ));
    }
    Ok(path)
}

fn write_report_atomically(
    final_path: &Path,
    overwrite: bool,
    write_fn: impl FnOnce(&mut std::fs::File) -> Result<(), String>,
) -> Result<(), String> {
    let parent = final_path
        .parent()
        .ok_or_else(|| "report output path must have a parent directory".to_string())?;
    let temp_name = format!(
        ".{}.{}.tmp",
        final_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("report"),
        Uuid::new_v4()
    );
    let temp_path = parent.join(temp_name);
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| e.to_string())?;

    let write_result = write_fn(&mut temp_file)
        .and_then(|_| temp_file.flush().map_err(|e| e.to_string()))
        .and_then(|_| temp_file.sync_all().map_err(|e| e.to_string()));

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    drop(temp_file);

    if overwrite && final_path.exists() {
        fs::remove_file(final_path).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            e.to_string()
        })?;
    }

    fs::rename(&temp_path, final_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        e.to_string()
    })
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
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(bundle_dir.join("manifest.json")).unwrap(),
        )
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
