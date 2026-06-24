use super::{
    correlation_confidence_str, current_analysis, current_correlation, current_governance, html,
    persist_report_record, prepare_report_output, write_report_atomically, ReportError,
};
use reports::CsvExporter;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;
use uuid::Uuid;

pub fn generate_csv_artifacts(
    conn: &Connection,
    case_id: &str,
    output_dir: &Path,
    scope: &ExportScopeDto,
) -> Result<String, ReportError> {
    let mut stmt = conn.prepare(
        "SELECT artifact_type, title, summary, extractor_id, extractor_version, confidence, source_attribution FROM artifacts ORDER BY created_at DESC LIMIT 1000"
    )?;
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
        })?
        .collect::<Result<Vec<Vec<String>>, rusqlite::Error>>()?;
    let mut rows_data = rows_data;
    let analysis = current_analysis(conn)?;
    let governance = current_governance(conn, case_id)?;
    let correlation = current_correlation(conn)?;
    rows_data.extend(
        html::report_analysis_rows(conn, case_id, &analysis, scope)
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
    rows_data.extend(
        html::report_governance_rows(&governance, scope)
            .into_iter()
            .map(|row| {
                vec![
                    "governance".to_string(),
                    "snapshot".to_string(),
                    row,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]
            }),
    );
    rows_data.extend(
        html::report_correlation_rows(&correlation, scope)
            .into_iter()
            .map(|row| {
                vec![
                    "correlation".to_string(),
                    "lead".to_string(),
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
        .map_err(|e| ReportError::Other(e.to_string()))
    })?;

    persist_report_record(conn, case_id, "report-files", &file_name, "completed")?;
    Ok(file_name)
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
    let path = prepare_report_output(output_dir, &file_name, scope.overwrite)?;
    write_report_atomically(&path, scope.overwrite, |file| {
        CsvExporter::export_correlation_leads(file, &rows)
            .map_err(|e| ReportError::Other(e.to_string()))
    })?;

    persist_report_record(conn, case_id, "report-correlation", &file_name, "completed")?;
    Ok(file_name)
}
