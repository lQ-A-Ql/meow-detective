use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::AnalysisFileClassificationDto;

use super::source::open_ready_analysis_source;
use crate::analysis_service::{classify_files_by_metadata, AnalysisServiceError};

pub fn classify_source_files(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    sample_size: u32,
) -> Result<Vec<AnalysisFileClassificationDto>, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    classify_files_by_metadata(&source.connection, sample_size)
}
