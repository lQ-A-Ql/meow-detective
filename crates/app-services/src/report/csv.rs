use super::{
    correlation_confidence_str, current_analysis, current_analysis_for_case, current_correlation,
    current_correlation_for_case, current_governance, execute_report_export, html,
    prepare_report_output, write_report_atomically, ReportError,
};
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use reports::CsvExporter;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;
use uuid::Uuid;

#[cfg(test)]
#[path = "../../tests/unit/report/artifact_export_identity_test.rs"]
mod tests;

const ARTIFACT_HEADERS: &[&str] = &[
    "id",
    "dataSourceId",
    "type",
    "title",
    "summary",
    "sourceObjectId",
    "extractorId",
    "extractorVersion",
    "confidence",
    "sourceAttribution",
];

pub fn generate_csv_artifacts(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let rows_data: Vec<Vec<String>> = ArtifactRepo::new(conn)
        .list_all_records()?
        .into_iter()
        .map(|record| {
            vec![
                record.id,
                record.data_source_id,
                record.artifact_type,
                record
                    .attrs
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                record
                    .attrs
                    .get("summary")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                record.source_object_id.unwrap_or_default(),
                record
                    .attrs
                    .get("extractorId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                record
                    .attrs
                    .get("extractorVersion")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                record
                    .attrs
                    .get("confidence")
                    .and_then(serde_json::Value::as_f64)
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                record
                    .attrs
                    .get("sourceAttribution")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ]
        })
        .collect();
    let mut rows_data = rows_data;
    let analysis = current_analysis(conn)?;
    let governance = current_governance(conn, case_id)?;
    let correlation = current_correlation(conn)?;
    append_context_rows(
        &mut rows_data,
        super::analysis_rows::report_analysis_rows(conn, case_id, &analysis, scope),
        html::report_governance_rows(&governance, scope),
        html::report_correlation_rows(&correlation, scope),
    );

    let file_name = format!("artifacts-{}.csv", Uuid::new_v4());
    execute_report_export(conn, case_id, "report-files", &file_name, || {
        let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
        write_report_atomically(&path, scope.overwrite, |file| {
            CsvExporter::export_artifacts(file, ARTIFACT_HEADERS, &rows_data)
                .map_err(|e| ReportError::Other(e.to_string()))
        })
    })?;
    Ok(file_name)
}

pub fn generate_csv_artifacts_for_case(
    conn: &Connection,
    case: &domain::CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let artifacts =
        crate::artifact_service::get_artifact_rows_for_case(conn, case_root, &case.id, None)?;
    let mut rows_data = artifacts
        .iter()
        .map(source_artifact_row)
        .collect::<Result<Vec<_>, _>>()?;
    let analysis = current_analysis_for_case(conn, case_root, &case.id)?;
    let governance = super::current_governance_for_case(conn, case_root, &case.id.0)?;
    let correlation = current_correlation_for_case(conn, case_root, &case.id)?;
    append_context_rows(
        &mut rows_data,
        super::analysis_rows::report_analysis_rows(conn, &case.id.0, &analysis, scope),
        html::report_governance_rows(&governance, scope),
        html::report_correlation_rows(&correlation, scope),
    );

    let file_name = format!("artifacts-{}.csv", Uuid::new_v4());
    execute_report_export(conn, &case.id.0, "report-files", &file_name, || {
        let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
        write_report_atomically(&path, scope.overwrite, |file| {
            CsvExporter::export_artifacts(file, ARTIFACT_HEADERS, &rows_data)
                .map_err(|e| ReportError::Other(e.to_string()))
        })
    })?;
    Ok(file_name)
}

fn source_artifact_row(
    artifact: &transport::dto::ArtifactRowDto,
) -> Result<Vec<String>, ReportError> {
    let data_source_id = super::source_identity::artifact_data_source_id(artifact)?;
    Ok(vec![
        artifact.id.clone(),
        data_source_id.0,
        artifact.artifact_type.clone(),
        artifact.title.clone(),
        artifact.summary.clone(),
        artifact.source_object_id.clone().unwrap_or_default(),
        artifact.extractor_id.clone().unwrap_or_default(),
        artifact.extractor_version.clone().unwrap_or_default(),
        artifact
            .confidence
            .map(|value| value.to_string())
            .unwrap_or_default(),
        artifact.source_attribution.clone().unwrap_or_default(),
    ])
}

fn append_context_rows(
    rows: &mut Vec<Vec<String>>,
    analysis: Vec<String>,
    governance: Vec<String>,
    correlation: Vec<String>,
) {
    for (kind, title, values) in [
        ("analysis", "provenance", analysis),
        ("governance", "snapshot", governance),
        ("correlation", "lead", correlation),
    ] {
        rows.extend(values.into_iter().map(|value| {
            vec![
                String::new(),
                String::new(),
                kind.to_string(),
                title.to_string(),
                value,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]
        }));
    }
}

pub fn generate_csv_correlation(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let correlation = current_correlation(conn)?;

    let rows: Vec<Vec<String>> = correlation
        .snapshot
        .leads
        .iter()
        .map(|lead| {
            let families = lead.families.join("; ");
            let provenance_sources = lead
                .provenance
                .iter()
                .map(|item| {
                    format!(
                        "{}:{}:{}",
                        item.source_kind, item.source_record_id, item.source_label
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let caveats = lead.caveats.join("; ");

            vec![
                lead.id.clone(),
                lead.title.clone(),
                correlation_confidence_str(&lead.confidence).to_string(),
                families,
                lead.primary_file_id.clone(),
                lead.supporting_node_ids.len().to_string(),
                lead.match_signals.len().to_string(),
                provenance_sources,
                caveats,
            ]
        })
        .collect();

    let file_name = format!("correlation-{}.csv", Uuid::new_v4());
    execute_report_export(conn, case_id, "report-correlation", &file_name, || {
        let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
        write_report_atomically(&path, scope.overwrite, |file| {
            CsvExporter::export_correlation_leads(file, &rows)
                .map_err(|e| ReportError::Other(e.to_string()))
        })
    })?;
    Ok(file_name)
}

pub fn generate_csv_correlation_for_case(
    conn: &Connection,
    case: &domain::CaseMeta,
    case_root: &Path,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let correlation = current_correlation_for_case(conn, case_root, &case.id)?;
    write_correlation_csv(conn, &case.id.0, output_dir, scope, &correlation)
}

fn write_correlation_csv(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
    correlation: &super::ReportCorrelation,
) -> Result<String, ReportError> {
    let _ = scope;

    let rows: Vec<Vec<String>> = correlation
        .snapshot
        .leads
        .iter()
        .map(|lead| {
            let families = lead.families.join("; ");
            let provenance_sources = lead
                .provenance
                .iter()
                .map(|item| {
                    format!(
                        "{}:{}:{}",
                        item.source_kind, item.source_record_id, item.source_label
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let caveats = lead.caveats.join("; ");

            vec![
                lead.id.clone(),
                lead.title.clone(),
                correlation_confidence_str(&lead.confidence).to_string(),
                families,
                lead.primary_file_id.clone(),
                lead.supporting_node_ids.len().to_string(),
                lead.match_signals.len().to_string(),
                provenance_sources,
                caveats,
            ]
        })
        .collect();

    let file_name = format!("correlation-{}.csv", Uuid::new_v4());
    execute_report_export(conn, case_id, "report-correlation", &file_name, || {
        let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
        write_report_atomically(&path, scope.overwrite, |file| {
            CsvExporter::export_correlation_leads(file, &rows)
                .map_err(|e| ReportError::Other(e.to_string()))
        })
    })?;
    Ok(file_name)
}
