use serde_json::{json, Value};
use transport::commands::ExportScopeDto;

use super::{ReportAnalysis, ReportSourceAnalysis};

pub(crate) fn analysis_json_section(analysis: &ReportAnalysis, scope: &ExportScopeDto) -> Value {
    match analysis {
        ReportAnalysis::Single {
            system_info,
            classifications,
        } => {
            let summary =
                crate::analysis_service::generate_analysis_summary(system_info, classifications);
            json!({
                "systemInfo": scope.registry.then_some(system_info),
                "classifications": if scope.file_system_metadata {
                    classifications.as_slice()
                } else {
                    &[]
                },
                "summary": summary,
            })
        }
        ReportAnalysis::PerSource(sources) => {
            let warnings = if scope.registry
                && super::source_analysis::unavailable_windows_system_info(analysis)
            {
                vec!["Windows system information is unavailable because the case has no ready Windows data source."]
            } else {
                Vec::new()
            };
            json!({
                "sources": sources
                    .iter()
                    .map(|source| source_json(source, scope))
                    .collect::<Vec<_>>(),
                "warnings": warnings,
            })
        }
    }
}

fn source_json(source: &ReportSourceAnalysis, scope: &ExportScopeDto) -> Value {
    let system_info = source.system_info.as_ref().filter(|_| scope.registry);
    let classifications = if scope.file_system_metadata {
        source.classifications.as_slice()
    } else {
        &[]
    };
    let summary = system_info.map(|system_info| {
        crate::analysis_service::generate_analysis_summary(system_info, classifications)
    });
    json!({
        "dataSourceId": source.data_source_id.0,
        "platform": source.platform.as_storage_str(),
        "systemInfo": system_info,
        "classifications": classifications,
        "summary": summary,
    })
}
