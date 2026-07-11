use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::AnalysisSystemInfoDto;

use super::source::open_ready_analysis_source;
use crate::analysis_service::{
    extract_system_info_for_case, validate_analysis_categories, AnalysisServiceError,
};
use crate::file_service::FileHeaderReadCache;

pub fn get_source_system_info(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<AnalysisSystemInfoDto, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &["Registry"])?;
    let header_cache = FileHeaderReadCache::new(case_id.0.clone());
    Ok(extract_system_info_for_case(
        &source.connection,
        |file_id, max_bytes| {
            header_cache.read_file_header_by_id(&source.connection, file_id, max_bytes)
        },
    ))
}
