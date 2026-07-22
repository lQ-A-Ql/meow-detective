use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;

use super::source::open_ready_analysis_source;
use crate::analysis_service::extraction::{
    run_analysis_extraction_with_bytes_and_cancel_and_progress, AnalysisExtractionExecution,
    ExtractionProgressUpdate,
};
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::{FileServiceError, SourceReadContext};
use transport::dto::{AnalysisExtractionProgressDto, AnalysisExtractionRunDto};

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

pub fn run_source_analysis_extraction_with_progress(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
    run_id: &str,
    mut progress: impl FnMut(AnalysisExtractionProgressDto),
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let mut emit = |update: ExtractionProgressUpdate| {
        progress(AnalysisExtractionProgressDto {
            run_id: run_id.to_string(),
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
            category: update.category,
            label: update.label,
            phase: update.phase,
            total_candidates: update.total_candidates,
            processed_candidates: update.processed_candidates,
            structured_candidates: update.structured_candidates,
            unsupported_candidates: update.unsupported_candidates,
            text_fallback_candidates: update.text_fallback_candidates,
            warning_candidates: update.warning_candidates,
            checkpoint_hit_count: update.checkpoint_hit_count,
            artifact_count: update.artifact_count,
            timeline_event_count: update.timeline_event_count,
            current_path: update.current_path,
            detail: update.detail,
        });
    };
    run_source_analysis_extraction_execution_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        cancel_token,
        &mut emit,
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
    let mut ignore_progress = |_update: ExtractionProgressUpdate| {};
    run_source_analysis_extraction_execution_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        cancel_token,
        &mut ignore_progress,
    )
}

fn run_source_analysis_extraction_execution_with_progress(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
    cancel_token: Arc<AtomicBool>,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
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
    let mut extraction = run_analysis_extraction_with_bytes_and_cancel_and_progress(
        &source.connection,
        &case_id.0,
        source.platform,
        categories,
        cancel_token.as_ref(),
        progress,
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
