pub mod csv;
pub mod html;
pub mod json;

#[cfg(test)]
mod tests;

pub use csv::{generate_csv_artifacts, generate_csv_correlation};
pub use json::generate_json_export;

use domain::CaseMeta;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
    report_repo::ReportRepo, timeline_repo::TimelineRepo,
};
use reports::HtmlReportExporter;
use rusqlite::Connection;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use transport::commands::ExportScopeDto;
use transport::dto::{
    AnalysisFileClassificationDto, AnalysisSystemInfoDto, CorrelationSnapshotDto,
    ReportHistoryItemDto, ReportTemplateDto, V2GovernanceSnapshotDto,
};
use uuid::Uuid;

pub(crate) struct ReportAnalysis {
    pub(crate) system_info: AnalysisSystemInfoDto,
    pub(crate) classifications: Vec<AnalysisFileClassificationDto>,
}

pub(crate) struct ReportCorrelation {
    pub(crate) snapshot: CorrelationSnapshotDto,
}

pub(crate) struct ReportGovernance {
    pub(crate) snapshot: V2GovernanceSnapshotDto,
}

pub(crate) struct RawExportBundle {
    pub(crate) bundle_dir_name: String,
    pub(crate) manifest_file_name: String,
    pub(crate) hashes_file_name: String,
    pub(crate) exported_count: usize,
}

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
            .map(|artifact| html::format_artifact_report_row(&artifact))
            .collect::<Vec<_>>();
        rows.push(format!("{} timeline events", tl_count));
        rows.extend(
            TimelineRepo::new(conn)
                .query(0, 500)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|event| html::format_timeline_report_row(&event)),
        );
        rows
    } else {
        Vec::new()
    };
    let analysis = current_analysis(conn)?;
    let analysis_rows = html::report_analysis_rows(conn, &case.id.0, &analysis, scope);
    let governance = current_governance(conn, &case.id.0)?;
    let governance_rows = html::report_governance_rows(&governance, scope);
    let correlation = current_correlation(conn)?;
    let correlation_rows = html::report_correlation_rows(&correlation, scope);
    let correlation_leads = html::report_correlation_lead_sections(&correlation, scope);

    let file_name = format!("report-{}.html", Uuid::new_v4());
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    write_report_atomically(&path, scope.overwrite, |file| {
        HtmlReportExporter::export_with_structured_sections(
            file,
            case,
            &files,
            &artifacts,
            &analysis_rows,
            &governance_rows,
            &correlation_rows,
            &correlation_leads,
        )
        .map_err(|e| e.to_string())
    })?;

    persist_report_record(conn, &case.id.0, "report-summary", &file_name, "completed")?;
    Ok(file_name)
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

pub(crate) fn current_analysis(conn: &Connection) -> Result<ReportAnalysis, String> {
    let system_info =
        crate::analysis_service::extract_system_info_for_case(conn, |file_id, max_bytes| {
            crate::file_service::read_file_header_by_id(conn, file_id, max_bytes)
        });
    let files = crate::analysis_service::collect_file_entries(conn).map_err(|e| e.to_string())?;
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

pub(crate) fn current_correlation(conn: &Connection) -> Result<ReportCorrelation, String> {
    Ok(ReportCorrelation {
        snapshot: crate::correlation::get_correlation_snapshot(conn)?,
    })
}

pub(crate) fn current_governance(
    conn: &Connection,
    case_id: &str,
) -> Result<ReportGovernance, String> {
    Ok(ReportGovernance {
        snapshot: crate::v2_governance_service::get_v2_governance_snapshot(conn, case_id)
            .map_err(|e| e.to_string())?,
    })
}

pub(crate) fn correlation_confidence_str(
    value: &transport::dto::CorrelationConfidenceDto,
) -> &'static str {
    match value {
        transport::dto::CorrelationConfidenceDto::Direct => "direct",
        transport::dto::CorrelationConfidenceDto::Strong => "strong",
        transport::dto::CorrelationConfidenceDto::Weak => "weak",
        transport::dto::CorrelationConfidenceDto::Heuristic => "heuristic",
    }
}

pub(crate) fn prepare_report_output(
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

pub(crate) fn write_report_atomically(
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

pub(crate) fn persist_report_record(
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

pub(crate) fn report_scope_warnings(
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

pub(crate) fn report_warnings(
    conn: &Connection,
    case_id: &str,
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = report_scope_warnings(scope, raw_bundle);
    warnings.extend(evidence_hash_warnings(conn, case_id));
    warnings
}

pub(crate) fn evidence_hash_warnings(conn: &Connection, case_id: &str) -> Vec<String> {
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

pub(crate) fn evidence_hash_warning_messages(
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
