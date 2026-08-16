use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;

use super::source::open_ready_analysis_source;
use crate::analysis_service::extraction::{
    encrypted_candidate_warning, run_analysis_extraction_with_source_and_progress,
    AnalysisExtractionExecution, CandidateSource, ExtractionProgressUpdate, PluginExtractFailure,
    PluginLoadRecord,
};
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::{
    FileServiceError, RangeContentReader, SourceReadContext, SourceReadFileHint,
};
use crate::plugin_loader::PluginRejection;
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use transport::dto::{AnalysisExtractionProgressDto, AnalysisExtractionRunDto};

use super::runtime::AnalysisSourceReadRuntime;

pub struct AnalysisExtractionProgressContext<'a> {
    pub source_runtime: &'a AnalysisSourceReadRuntime,
    pub run_id: &'a str,
}

impl<'a> AnalysisExtractionProgressContext<'a> {
    #[must_use]
    pub fn new(source_runtime: &'a AnalysisSourceReadRuntime, run_id: &'a str) -> Self {
        Self {
            source_runtime,
            run_id,
        }
    }
}

struct AnalysisExecutionControl<'a> {
    cancel_token: Arc<AtomicBool>,
    source_runtime: &'a AnalysisSourceReadRuntime,
    progress: &'a mut dyn FnMut(ExtractionProgressUpdate),
}

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
    context: AnalysisExtractionProgressContext<'_>,
    mut progress: impl FnMut(AnalysisExtractionProgressDto),
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let mut emit = |update: ExtractionProgressUpdate| {
        progress(AnalysisExtractionProgressDto {
            run_id: context.run_id.to_string(),
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
    let control = AnalysisExecutionControl {
        cancel_token,
        source_runtime: context.source_runtime,
        progress: &mut emit,
    };
    run_source_analysis_extraction_execution_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        control,
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
    let runtime = AnalysisSourceReadRuntime::default();
    let mut ignore_progress = |_update: ExtractionProgressUpdate| {};
    let control = AnalysisExecutionControl {
        cancel_token,
        source_runtime: &runtime,
        progress: &mut ignore_progress,
    };
    run_source_analysis_extraction_execution_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        categories,
        control,
    )
}

fn run_source_analysis_extraction_execution_with_progress(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
    control: AnalysisExecutionControl<'_>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    // If a WeChat key recovery previously produced keys for this case, point
    // the plugin's injection channel at them for this run; no keys file means
    // behavior is unchanged (see WeChatKeysEnvGuard for the process-level env
    // concurrency argument).
    let _wechat_keys = crate::wechat_key_service::WeChatKeysEnvGuard::activate(case_root);
    let mut source_reader = control.source_runtime.bind(SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    ));
    let mut source_read_count = 0u64;
    let mut source_read_elapsed = Duration::ZERO;
    let mut extraction = run_analysis_extraction_with_source_and_progress(
        &source.connection,
        &case_id.0,
        source.platform,
        categories,
        control.cancel_token.as_ref(),
        control.progress,
        |candidate, read_limit| -> Result<CandidateSource, FileServiceError> {
            if let Some(warning) = encrypted_candidate_warning(candidate) {
                return Err(FileServiceError::Unsupported(warning));
            }
            let started = Instant::now();
            let result = if candidate.category == "EventLogs" {
                source_reader
                    .open_file_range_by_id(&candidate.file_id)
                    .map(candidate_source_from_range_reader)
            } else {
                source_reader
                    .read_file_header_with_metadata(
                        SourceReadFileHint::new(
                            candidate.file_id.clone(),
                            DataSourceId(candidate.data_source_id.clone()),
                            candidate.partition_index,
                            candidate.path.clone(),
                            candidate.size,
                            candidate.encrypted,
                        ),
                        read_limit,
                    )
                    .map(CandidateSource::Bytes)
            };
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
        record_plugin_audit_trail(
            case_conn,
            case_id,
            data_source_id,
            &execution.plugin_loads,
            &execution.plugin_rejections,
            &execution.plugin_extract_failures,
        );
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

fn candidate_source_from_range_reader(reader: RangeContentReader) -> CandidateSource {
    match reader {
        RangeContentReader::Seekable(reader) => CandidateSource::Seekable(reader),
        RangeContentReader::Streaming(reader) => CandidateSource::Reader(reader),
    }
}

/// Write the plugin audit trail of one extraction run into the case audit
/// log (design doc §5: plugin load / refusal / extraction failure). Audit
/// writes are non-fatal: a failing audit insert must never fail the run.
fn record_plugin_audit_trail(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    plugin_loads: &[PluginLoadRecord],
    plugin_rejections: &[PluginRejection],
    plugin_extract_failures: &[PluginExtractFailure],
) {
    if plugin_loads.is_empty() && plugin_rejections.is_empty() && plugin_extract_failures.is_empty()
    {
        return;
    }
    let repo = AuditRepo::new(case_conn);
    for load in plugin_loads {
        let details = serde_json::json!({
            "pluginId": load.plugin_id,
            "pluginVersion": load.plugin_version,
            "dataSourceId": data_source_id.0,
        })
        .to_string();
        log_audit_outcome(repo.log(
            Some(&case_id.0),
            "system",
            &AuditAction::PluginLoad,
            Some(&load.plugin_id),
            &details,
        ));
    }
    for rejection in plugin_rejections {
        let details = serde_json::json!({
            "path": rejection.path.display().to_string(),
            "reason": rejection.reason,
            "dataSourceId": data_source_id.0,
        })
        .to_string();
        log_audit_outcome(repo.log(
            Some(&case_id.0),
            "system",
            &AuditAction::PluginReject,
            None,
            &details,
        ));
    }
    for failure in plugin_extract_failures {
        let details = serde_json::json!({
            "pluginId": failure.plugin_id,
            "path": failure.source_path,
            "error": failure.error,
            "dataSourceId": data_source_id.0,
        })
        .to_string();
        log_audit_outcome(repo.log(
            Some(&case_id.0),
            "system",
            &AuditAction::PluginExtractFailed,
            Some(&failure.plugin_id),
            &details,
        ));
    }
}

fn log_audit_outcome(result: persistence_sqlite::DbResult<()>) {
    if let Err(error) = result {
        tracing::warn!("plugin audit event could not be recorded: {error}");
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/use_cases/extraction.rs"]
mod tests;
