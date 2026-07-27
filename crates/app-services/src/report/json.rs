use super::{
    current_analysis, current_analysis_for_case, current_correlation, current_correlation_for_case,
    current_governance, persist_report_record, prepare_report_output, write_report_atomically,
    BitLockerReportContext, RawExportBundle, ReportCorrelation, ReportError, ReportGovernance,
};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use reports::JsonExporter;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;
use uuid::Uuid;

pub(crate) mod raw_bundle;

use raw_bundle::{export_raw_file_bundle, export_raw_file_bundle_for_case};

// ---------------------------------------------------------------------------
// Public JSON export
// ---------------------------------------------------------------------------

pub fn generate_json_export(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let events = if scope.full_timeline {
        TimelineRepo::new(conn).query(0, 500)?
    } else {
        Vec::new()
    };
    let artifacts = ArtifactRepo::new(conn).list_by_family(None)?;
    let analysis = current_analysis(conn)?;
    let governance = current_governance(conn, case_id)?;
    let correlation = current_correlation(conn)?;
    let analysis_section = super::analysis_json::analysis_json_section(&analysis, scope);
    let json_val = serde_json::json!({
        "timeline_events": events.iter().map(super::json_records::legacy_timeline_event).collect::<Vec<_>>(),
        "artifacts": artifacts.iter().map(super::json_records::legacy_artifact).collect::<Vec<_>>(),
        "scope": scope,
        "warnings": serde_json::Value::Array(Vec::new()),
        "analysis": analysis_section,
        "governance": governance_json_section(&governance),
        "correlation": correlation_json_section(&correlation),
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
    let warnings = super::report_warnings(conn, case_id, scope, raw_bundle.as_ref());
    write_report_atomically(&path, scope.overwrite, |file| {
        let mut payload = json_val;
        if let Some(bundle) = &raw_bundle {
            payload["rawExport"] = serde_json::json!({
                "bundleDirectory": bundle.bundle_dir_name,
                "manifestFile": bundle.manifest_file_name,
                "hashesFile": bundle.hashes_file_name,
                "exportedCount": bundle.exported_count,
                "skippedCount": bundle.skipped_count,
            });
        }
        payload["warnings"] =
            serde_json::to_value(&warnings).map_err(|e| ReportError::Other(e.to_string()))?;
        JsonExporter::export(file, &payload).map_err(|e| ReportError::Other(e.to_string()))
    })?;

    persist_report_record(conn, case_id, "report-summary", &file_name, "completed")?;
    Ok(file_name)
}

pub fn generate_json_export_for_case(
    conn: &Connection,
    case: &domain::CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    generate_json_export_for_case_with_context(conn, case, case_root, output_dir, scope, None)
}

pub fn generate_json_export_for_case_with_bitlocker(
    conn: &Connection,
    case: &domain::CaseMeta,
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
    case: &domain::CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
    bitlocker_context: Option<BitLockerReportContext<'_>>,
) -> Result<String, ReportError> {
    let events = if scope.full_timeline {
        crate::timeline_service::query_timeline_for_case(conn, case_root, &case.id, 0, 500)?.items
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
        "governance": governance_json_section(&governance),
        "correlation": correlation_json_section(&correlation),
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

// ---------------------------------------------------------------------------
// JSON section helpers
// ---------------------------------------------------------------------------

pub(crate) fn correlation_json_section(correlation: &ReportCorrelation) -> serde_json::Value {
    serde_json::json!({
        "generatedAt": correlation.snapshot.generated_at,
        "leadCount": correlation.snapshot.lead_count,
        "clusterCount": correlation.snapshot.cluster_count,
        "nodeCount": correlation.snapshot.node_count,
        "edgeCount": correlation.snapshot.edge_count,
        "familyCoverage": correlation.snapshot.family_coverage,
        "leads": correlation.snapshot.leads.iter().map(|lead| serde_json::json!({
            "id": lead.id,
            "title": lead.title,
            "summary": lead.summary,
            "confidence": lead.confidence,
            "families": lead.families,
            "primaryFileId": lead.primary_file_id,
            "supportingNodeIds": lead.supporting_node_ids,
            "matchSignals": lead.match_signals,
            "jumps": lead.jumps,
            "provenance": lead.provenance,
            "caveats": lead.caveats,
        })).collect::<Vec<_>>(),
    })
}

pub(crate) fn governance_json_section(governance: &ReportGovernance) -> serde_json::Value {
    let snapshot = &governance.snapshot;
    serde_json::json!({
        "generatedAt": snapshot.generated_at,
        "factSources": snapshot.fact_sources,
        "runtimeResults": snapshot.runtime_results,
        "verificationChains": snapshot.verification_chains,
        "supportMatrix": snapshot.support_matrix,
        "supportMatrixEntries": snapshot.support_matrix_entries,
        "knownLimitations": snapshot.known_limitations,
        "benchmark": snapshot.benchmark,
        "security": snapshot.security,
        "errorTaxonomyEntries": snapshot.error_taxonomy_entries,
        "releaseGates": snapshot.release_gates,
        "releaseScorecard": snapshot.release_scorecard,
        "runtimeSignals": snapshot.runtime_signals,
    })
}
