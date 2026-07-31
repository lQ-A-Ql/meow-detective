use super::{
    current_analysis_for_case, current_correlation_for_case, persist_report_record,
    prepare_report_output, write_report_atomically, BitLockerReportContext, ReportError,
};
use domain::CaseMeta;
use reports::JsonExporter;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;
use uuid::Uuid;

use super::json::raw_bundle::export_raw_file_bundle_for_case;

pub fn generate_json_export_for_case(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    generate_json_export_for_case_with_context(conn, case, case_root, output_dir, scope, None)
}

pub fn generate_json_export_for_case_with_bitlocker(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
    bitlocker_context: BitLockerReportContext<'_>,
) -> Result<String, ReportError> {
    generate_json_export_for_case_with_context(
        conn,
        case,
        case_root,
        output_dir,
        scope,
        Some(bitlocker_context),
    )
}

fn generate_json_export_for_case_with_context(
    conn: &Connection,
    case: &CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
    bitlocker_context: Option<BitLockerReportContext<'_>>,
) -> Result<String, ReportError> {
    let events = if scope.full_timeline {
        super::load_full_timeline_for_case(conn, case_root, &case.id)?
    } else {
        Vec::new()
    };
    let artifacts =
        crate::artifact_service::get_artifact_rows_for_case(conn, case_root, &case.id, None)?;
    let analysis = current_analysis_for_case(conn, case_root, &case.id)?;
    let governance = super::current_governance_for_case(conn, case_root, &case.id.0)?;
    let correlation = current_correlation_for_case(conn, case_root, &case.id)?;
    let analysis_section = super::analysis_json::analysis_json_section(&analysis, scope);
    let bitlocker =
        super::bitlocker::current_inventory(conn, case_root, &case.id, scope, bitlocker_context)?;
    let timeline_events = events
        .iter()
        .map(super::json_records::source_timeline_event)
        .collect::<Result<Vec<_>, _>>()?;
    let artifact_rows = artifacts
        .iter()
        .map(super::json_records::source_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    let mut json_val = serde_json::json!({
        "timeline_events": timeline_events,
        "artifacts": artifact_rows,
        "scope": scope,
        "warnings": serde_json::Value::Array(Vec::new()),
        "analysis": analysis_section,
        "governance": super::json::governance_json_section(&governance),
        "correlation": super::json::correlation_json_section(&correlation),
        "bitlocker": super::bitlocker::json_section(&bitlocker),
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    let raw_bundle = if scope.raw_file_extraction {
        Some(export_raw_file_bundle_for_case(
            conn,
            case_root,
            output_dir,
            &case.id.0,
            &file_name,
            scope.overwrite,
            bitlocker_context.map(BitLockerReportContext::unlock_registry),
        )?)
    } else {
        None
    };
    let warnings = super::report_warnings(conn, &case.id.0, scope, raw_bundle.as_ref());
    write_report_atomically(&path, scope.overwrite, |file| {
        if let Some(bundle) = &raw_bundle {
            json_val["rawExport"] = serde_json::json!({
                "bundleDirectory": bundle.bundle_dir_name,
                "manifestFile": bundle.manifest_file_name,
                "hashesFile": bundle.hashes_file_name,
                "exportedCount": bundle.exported_count,
                "skippedCount": bundle.skipped_count,
            });
        }
        json_val["warnings"] =
            serde_json::to_value(&warnings).map_err(|e| ReportError::Other(e.to_string()))?;
        JsonExporter::export(file, &json_val).map_err(|e| ReportError::Other(e.to_string()))
    })?;

    persist_report_record(conn, &case.id.0, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}
