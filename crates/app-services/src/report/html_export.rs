use super::{
    analysis_rows, current_analysis, current_analysis_for_case, current_correlation,
    current_correlation_for_case, current_governance, current_governance_for_case, html,
    persist_report_record, prepare_report_output, source_analysis, write_report_atomically,
    BitLockerReportContext, ReportAnalysis, ReportError,
};
use domain::CaseMeta;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use reports::HtmlReportExporter;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;
use uuid::Uuid;

pub fn generate_html_report(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    generate_html_report_with_analysis(conn, case, output_dir, scope, || current_analysis(conn))
}

pub fn generate_html_report_for_case(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    generate_html_report_for_case_with_analysis(
        conn,
        case,
        case_root,
        output_dir,
        scope,
        None,
        || current_analysis_for_case(conn, case_root, &case.id),
    )
}

pub fn generate_html_report_for_case_with_bitlocker(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
    bitlocker_context: BitLockerReportContext<'_>,
) -> Result<String, ReportError> {
    generate_html_report_for_case_with_analysis(
        conn,
        case,
        case_root,
        output_dir,
        scope,
        Some(bitlocker_context),
        || current_analysis_for_case(conn, case_root, &case.id),
    )
}

fn generate_html_report_with_analysis(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
    scope: &ExportScopeDto,
    analysis_loader: impl FnOnce() -> Result<ReportAnalysis, ReportError>,
) -> Result<String, ReportError> {
    let file_count = FileRepo::new(conn).count_all().unwrap_or(0);
    let timeline_count = TimelineRepo::new(conn).count().unwrap_or(0);
    let files = file_summary(scope, file_count);
    let artifacts = legacy_timeline_rows(conn, scope, timeline_count);
    let analysis = analysis_loader()?;
    let analysis_rows = analysis_rows::report_analysis_rows(conn, &case.id.0, &analysis, scope);
    let governance = current_governance(conn, &case.id.0)?;
    let correlation = current_correlation(conn)?;
    write_html_report(
        conn,
        case,
        output_dir,
        scope,
        files,
        artifacts,
        analysis_rows,
        &governance,
        &correlation,
    )
}

fn generate_html_report_for_case_with_analysis(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
    bitlocker_context: Option<BitLockerReportContext<'_>>,
    analysis_loader: impl FnOnce() -> Result<ReportAnalysis, ReportError>,
) -> Result<String, ReportError> {
    let file_count = file_count_for_case(conn, case, case_root)?;
    let timeline_count = timeline_count_for_case(conn, case, case_root)?;
    let files = file_summary(scope, file_count);
    let artifacts = source_timeline_rows(conn, case, case_root, scope, timeline_count)?;
    let analysis = analysis_loader()?;
    let mut analysis_rows = analysis_rows::report_analysis_rows(conn, &case.id.0, &analysis, scope);
    let bitlocker =
        super::bitlocker::current_inventory(conn, case_root, &case.id, scope, bitlocker_context)?;
    analysis_rows.extend(super::bitlocker::report_rows(&bitlocker));
    let governance = current_governance_for_case(conn, case_root, &case.id.0)?;
    let correlation = current_correlation_for_case(conn, case_root, &case.id)?;
    write_html_report(
        conn,
        case,
        output_dir,
        scope,
        files,
        artifacts,
        analysis_rows,
        &governance,
        &correlation,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_html_report(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
    scope: &ExportScopeDto,
    files: Vec<String>,
    artifacts: Vec<String>,
    analysis_rows: Vec<String>,
    governance: &super::ReportGovernance,
    correlation: &super::ReportCorrelation,
) -> Result<String, ReportError> {
    let governance_rows = html::report_governance_rows(governance, scope);
    let correlation_rows = html::report_correlation_rows(correlation, scope);
    let correlation_leads = html::report_correlation_lead_sections(correlation, scope);
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
        .map_err(|err| ReportError::Other(err.to_string()))
    })?;
    persist_report_record(conn, &case.id.0, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}

fn file_summary(scope: &ExportScopeDto, file_count: u64) -> Vec<String> {
    if scope.file_system_metadata {
        vec![format!("{} files indexed", file_count)]
    } else {
        Vec::new()
    }
}

fn legacy_timeline_rows(
    conn: &Connection,
    scope: &ExportScopeDto,
    timeline_count: u64,
) -> Vec<String> {
    if !scope.full_timeline {
        return Vec::new();
    }
    let mut rows = ArtifactRepo::new(conn)
        .list_by_family(None)
        .unwrap_or_default()
        .into_iter()
        .map(|artifact| html::format_artifact_report_row(&artifact))
        .collect::<Vec<_>>();
    rows.push(format!("{} timeline events", timeline_count));
    rows.extend(
        TimelineRepo::new(conn)
            .query(0, 500)
            .unwrap_or_default()
            .into_iter()
            .map(|event| html::format_timeline_report_row(&event)),
    );
    rows
}

fn source_timeline_rows(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    scope: &ExportScopeDto,
    timeline_count: u64,
) -> Result<Vec<String>, ReportError> {
    if !scope.full_timeline {
        return Ok(Vec::new());
    }
    let mut rows =
        crate::artifact_service::get_artifact_rows_for_case(conn, case_root, &case.id, None)?
            .iter()
            .map(html::format_artifact_dto_report_row)
            .collect::<Result<Vec<_>, _>>()?;
    rows.push(format!("{} timeline events", timeline_count));
    rows.extend(
        crate::timeline_service::query_timeline_for_case(conn, case_root, &case.id, 0, 500)?
            .items
            .iter()
            .map(html::format_timeline_dto_report_row)
            .collect::<Result<Vec<_>, _>>()?,
    );
    Ok(rows)
}

fn file_count_for_case(
    case_conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
) -> Result<u64, ReportError> {
    let mut total = 0u64;
    for source in source_analysis::open_ready_source_connections(case_conn, case_root, &case.id)? {
        total = total.saturating_add(FileRepo::new(&source.connection).count_all()?);
    }
    Ok(total)
}

fn timeline_count_for_case(
    case_conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
) -> Result<u64, ReportError> {
    Ok(
        crate::timeline_service::query_timeline_for_case(case_conn, case_root, &case.id, 0, 1)?
            .total,
    )
}
