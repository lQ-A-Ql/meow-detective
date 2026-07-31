mod analysis_json;
mod analysis_rows;
mod bitlocker;
mod catalog;
pub mod csv;
pub mod error;
pub mod html;
mod html_export;
pub mod json;
mod json_case;
mod json_records;
mod output;
mod snapshot;
mod source_analysis;
mod source_identity;
mod timeline_rows;
mod types;
mod warnings;

pub use catalog::{get_report_history, get_report_templates};
pub use csv::{
    generate_csv_artifacts, generate_csv_artifacts_for_case, generate_csv_correlation,
    generate_csv_correlation_for_case,
};
pub use error::ReportError;
pub use html_export::{
    generate_html_report, generate_html_report_for_case,
    generate_html_report_for_case_with_bitlocker,
};
pub use json::generate_json_export;
pub use json_case::{generate_json_export_for_case, generate_json_export_for_case_with_bitlocker};

pub(crate) use output::{persist_report_record, prepare_report_output, write_report_atomically};
pub(crate) use snapshot::{
    correlation_confidence_str, current_analysis, current_correlation,
    current_correlation_for_case, current_governance, current_governance_for_case,
    open_ready_source_connections,
};
pub(crate) use source_analysis::{current_analysis_for_case, ReportAnalysis, ReportSourceAnalysis};
pub(crate) use timeline_rows::load_full_timeline_for_case;
pub use types::BitLockerReportContext;
pub(crate) use types::{RawExportBundle, ReportCorrelation, ReportGovernance};
pub(crate) use warnings::{evidence_hash_warnings, report_scope_warnings, report_warnings};

#[cfg(test)]
#[path = "../../tests/unit/report/mod.rs"]
mod tests;
