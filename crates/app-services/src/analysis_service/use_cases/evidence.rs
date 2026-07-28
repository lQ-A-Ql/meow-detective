use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::EvidenceClassificationSummaryDto;

use super::source::open_ready_analysis_source;
use crate::analysis_service::{
    get_evidence_classification_summary, select_evidence_scan_categories, AnalysisServiceError,
};
use crate::{artifact_service, file_service};

use super::runtime::AnalysisSourceReadRuntime;

pub fn get_source_evidence_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    get_evidence_classification_summary(&source.connection, source.platform)
}

pub fn run_source_evidence_scan(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    requested_categories: &[&str],
    runtime: &AnalysisSourceReadRuntime,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    let categories = select_evidence_scan_categories(source.platform, requested_categories)?;
    let mut source_reader = runtime.bind(file_service::SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    ));
    artifact_service::run_targeted_evidence_scan(
        &source.connection,
        &case_id.0,
        &categories,
        |file_id| {
            source_reader
                .read_file_header_by_id(
                    file_id,
                    infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize,
                )
                .map(std::io::Cursor::new)
                .map(|cursor| Box::new(cursor) as Box<dyn std::io::Read>)
                .map_err(artifact_service::ArtifactServiceError::from)
        },
    )
    .map_err(|error| AnalysisServiceError::Extraction(error.to_string()))?;
    get_evidence_classification_summary(&source.connection, source.platform)
}
