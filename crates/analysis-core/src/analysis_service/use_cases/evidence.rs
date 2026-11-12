use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::EvidenceClassificationSummaryDto;

use super::source::open_ready_analysis_source_read_only;
use crate::analysis_service::{get_evidence_classification_summary, AnalysisServiceError};

pub fn get_source_evidence_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<EvidenceClassificationSummaryDto, AnalysisServiceError> {
    let source =
        open_ready_analysis_source_read_only(case_conn, case_root, case_id, data_source_id)?;
    get_evidence_classification_summary(&source.connection, source.platform)
}
