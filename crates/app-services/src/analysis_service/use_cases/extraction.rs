use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::AnalysisExtractionRunDto;

use super::source::open_ready_analysis_source;
use crate::analysis_service::extraction::{
    run_analysis_extraction_with_bytes_and_cancel, AnalysisExtractionExecution,
};
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::{FileServiceError, SourceReadContext};

pub fn run_source_analysis_extraction(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    run_source_analysis_extraction_with_cancel(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        cancel_token,
    )
}

pub fn run_source_analysis_extraction_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
    cancel_token: Arc<AtomicBool>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    run_source_analysis_extraction_execution_with_cancel(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        cancel_token,
    )
    .map(|execution| execution.dto)
}

pub(crate) fn run_source_analysis_extraction_execution_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
    cancel_token: Arc<AtomicBool>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    let mut source_reader = SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    );
    let mut source_read_count = 0u64;
    let mut source_read_elapsed = Duration::ZERO;
    let mut extraction = run_analysis_extraction_with_bytes_and_cancel(
        &source.connection,
        &case_id.0,
        source.platform,
        categories,
        cancel_token.as_ref(),
        |candidate, read_limit| -> Result<Vec<u8>, FileServiceError> {
            let started = Instant::now();
            let result = source_reader.read_file_header_with_metadata(
                &candidate.file_id,
                &DataSourceId(candidate.data_source_id.clone()),
                candidate.partition_index,
                &candidate.path,
                candidate.size,
                read_limit,
            );
            source_read_count = source_read_count.saturating_add(1);
            source_read_elapsed = source_read_elapsed.saturating_add(started.elapsed());
            result
        },
    );
    if let Ok(execution) = &mut extraction {
        execution.source_read_count = source_read_count;
        execution.source_read_elapsed_ms =
            u64::try_from(source_read_elapsed.as_millis()).unwrap_or(u64::MAX);
        execution.filesystem_read_metrics = source_reader.filesystem_read_metrics();
        execution.rados_read_metrics = source_reader.rados_read_metrics();
    }
    if extraction.is_ok() {
        if let Err(error) = source_reader.flush_derived_filesystem_locators() {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                error = %error,
                "Derived filesystem locator acceleration hints could not be persisted"
            );
        }
    }
    extraction
}
