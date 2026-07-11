use rusqlite::Connection;
use transport::commands::ExportScopeDto;
use transport::dto::{
    AnalysisFileClassificationDto, AnalysisParseStatusDto, AnalysisProvenanceDto,
    AnalysisSystemInfoDto,
};

use super::{ReportAnalysis, ReportSourceAnalysis};

pub(crate) fn report_analysis_rows(
    conn: &Connection,
    case_id: &str,
    analysis: &ReportAnalysis,
    scope: &ExportScopeDto,
) -> Vec<String> {
    let mut rows = scoped_analysis_rows(analysis, scope);
    rows.extend(super::evidence_hash_warnings(conn, case_id));
    rows
}

fn scoped_analysis_rows(analysis: &ReportAnalysis, scope: &ExportScopeDto) -> Vec<String> {
    let mut rows = super::report_scope_warnings(scope, None);
    match analysis {
        ReportAnalysis::Single {
            system_info,
            classifications,
        } => {
            if scope.registry {
                rows.extend(system_info_rows(system_info));
            }
            if scope.file_system_metadata {
                rows.extend(classification_rows(classifications));
            }
        }
        ReportAnalysis::PerSource(sources) => {
            if scope.registry && super::source_analysis::unavailable_windows_system_info(analysis) {
                rows.push(
                    "system_info status=unavailable warning=no ready Windows data source"
                        .to_string(),
                );
            }
            for source in sources {
                rows.extend(source_rows(source, scope));
            }
        }
    }
    rows
}

fn source_rows(source: &ReportSourceAnalysis, scope: &ExportScopeDto) -> Vec<String> {
    let prefix = format!(
        "dataSourceId={} platform={}",
        source.data_source_id.0, source.platform
    );
    let mut rows = vec![format!("analysis_source {prefix}")];
    if scope.registry {
        if let Some(system_info) = &source.system_info {
            rows.extend(
                system_info_rows(system_info)
                    .into_iter()
                    .map(|row| format!("{prefix} {row}")),
            );
        }
    }
    if scope.file_system_metadata {
        rows.extend(
            classification_rows(&source.classifications)
                .into_iter()
                .map(|row| format!("{prefix} {row}")),
        );
    }
    rows
}

pub(crate) fn system_info_rows(system_info: &AnalysisSystemInfoDto) -> Vec<String> {
    let mut rows = vec![format!(
        "system_info status={} warnings={}",
        status_str(&system_info.status),
        system_info.warnings.join(" | ")
    )];
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
    rows
}

fn classification_rows(classifications: &[AnalysisFileClassificationDto]) -> Vec<String> {
    let mut rows = Vec::new();
    for classification in classifications {
        rows.push(format!(
            "classification category={} files={} totalSize={} status={} warnings={}",
            classification.category,
            classification.file_count,
            classification.total_size,
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

fn status_str(status: &AnalysisParseStatusDto) -> &'static str {
    match status {
        AnalysisParseStatusDto::Parsed => "parsed",
        AnalysisParseStatusDto::Partial => "partial",
        AnalysisParseStatusDto::NotParsed => "notParsed",
        AnalysisParseStatusDto::Unavailable => "unavailable",
        AnalysisParseStatusDto::CandidateFound => "candidateFound",
        AnalysisParseStatusDto::NotFound => "notFound",
        AnalysisParseStatusDto::Failed => "failed",
    }
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
