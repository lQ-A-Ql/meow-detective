//! Source-scoped evidence scan hosted above `analysis-core`.
//!
//! The scan drives artifact extractors and the parser-plugin host, which stay
//! in this crate. `analysis-core` supplies the ready-source guard, the
//! platform category selection, and the runtime BitLocker binding used to feed
//! bounded header reads without opening a full source reader.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use analysis_core::analysis_service::{
    ensure_not_cancelled, get_evidence_classification_summary, open_ready_analysis_source,
    select_evidence_scan_categories, AnalysisServiceError, AnalysisSourceReadRuntime,
};
use domain::{CaseId, DataSourceId};
use transport::dto::EvidenceClassificationSummaryDto;

use super::{run_targeted_evidence_scan, ArtifactServiceError};
use crate::file_service::SourceReadContext;

pub fn run_source_evidence_scan(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    requested_categories: &[&str],
    runtime: &AnalysisSourceReadRuntime,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    run_source_evidence_scan_with_cancel(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        requested_categories,
        runtime,
        &AtomicBool::new(false),
    )
}

pub fn run_source_evidence_scan_with_cancel(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    requested_categories: &[&str],
    runtime: &AnalysisSourceReadRuntime,
    cancel_token: &AtomicBool,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    let categories = select_evidence_scan_categories(source.platform, requested_categories)?;
    let mut source_reader = runtime.bind(SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    ));
    run_targeted_evidence_scan(&source.connection, &case_id.0, &categories, |file_id| {
        source_reader
            .read_file_header_by_id(
                file_id,
                infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize,
            )
            .map(std::io::Cursor::new)
            .map(|cursor| Box::new(cursor) as Box<dyn std::io::Read>)
            .map_err(ArtifactServiceError::from)
    })
    .map_err(|error| AnalysisServiceError::Extraction(error.to_string()))?;
    ensure_not_cancelled(cancel_token)?;
    get_evidence_classification_summary(&source.connection, source.platform)
}
