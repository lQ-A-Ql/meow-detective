use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;

use super::source::open_ready_analysis_source;
use crate::analysis_service::{
    classify_files_by_metadata, extract_system_info_for_case, generate_analysis_summary,
    validate_analysis_categories, AnalysisServiceError, DEFAULT_SAMPLE_SIZE,
};
use crate::file_service::FileHeaderReadCache;

pub fn generate_source_analysis_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<String, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &["Registry"])?;
    let header_cache = FileHeaderReadCache::new(case_id.0.clone());
    let system_info = extract_system_info_for_case(&source.connection, |file_id, max_bytes| {
        header_cache.read_file_header_by_id(&source.connection, file_id, max_bytes)
    });
    let classifications = classify_files_by_metadata(&source.connection, DEFAULT_SAMPLE_SIZE)?;
    Ok(generate_analysis_summary(&system_info, &classifications))
}
