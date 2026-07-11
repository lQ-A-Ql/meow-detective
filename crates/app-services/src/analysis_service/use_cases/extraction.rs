use std::io::{Cursor, Read};
use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::AnalysisExtractionRunDto;

use super::source::open_ready_analysis_source;
use crate::analysis_service::{run_analysis_extraction, AnalysisServiceError};
use crate::file_service::{FileHeaderReadCache, FileServiceError};

pub fn run_source_analysis_extraction(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    let header_cache = FileHeaderReadCache::new(case_id.0.clone());
    run_analysis_extraction(
        &source.connection,
        &case_id.0,
        source.platform,
        categories,
        |file_id| -> Result<Box<dyn Read>, FileServiceError> {
            let bytes = header_cache.read_file_header_by_id(
                &source.connection,
                file_id,
                super::super::MAX_ANALYSIS_SOURCE_BYTES,
            )?;
            Ok(Box::new(Cursor::new(bytes)))
        },
    )
}
