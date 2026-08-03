use transport::dto::{
    ImportPhaseDto, ImportPhaseMetricsDto, ImportPhaseProgressDto, ImportPhaseStateDto,
};

use super::parsing::{
    profile_f64, profile_nonzero_u64, profile_u64, profile_value, rows_from_profile,
};
use super::results::{cache_statuses_from_profile, partial_results_from_profile};
use crate::import_pipeline::emit::ImportEventSink;

pub(crate) fn emit_phase_profile(
    event_sink: Option<&dyn ImportEventSink>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: String,
    cancel_requested: bool,
) {
    emit_import_profile_progress(
        event_sink,
        job_id,
        case_id,
        data_source_id,
        progress,
        &detail,
        cancel_requested,
    );
}

pub(crate) fn emit_import_profile_progress(
    event_sink: Option<&dyn ImportEventSink>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: &str,
    cancel_requested: bool,
) {
    tracing::info!("Import profile for {}: {}", job_id.0, detail);
    let phase_progress = import_phase_progress_from_profile(
        job_id,
        case_id,
        data_source_id,
        progress,
        detail,
        cancel_requested,
    );
    crate::import_pipeline::emit::emit_import_phase_progress(event_sink, &phase_progress);
    for result in &phase_progress.partial_results {
        crate::import_pipeline::emit::emit_import_partial_result(event_sink, result);
    }
    for status in cache_statuses_from_profile(data_source_id, detail) {
        crate::import_pipeline::emit::emit_cache_index_status(event_sink, &status);
    }
    crate::import_pipeline::emit::emit_job_progress(
        event_sink,
        &job_id.0,
        progress.min(99),
        detail,
    );
}

pub(crate) fn import_phase_progress_from_profile(
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: &str,
    cancel_requested: bool,
) -> ImportPhaseProgressDto {
    ImportPhaseProgressDto {
        job_id: job_id.0.clone(),
        case_id: case_id.0.clone(),
        data_source_id: data_source_id.map(|id| id.0.clone()),
        phase: import_phase_from_profile(detail, progress),
        state: import_phase_state_from_profile(detail, cancel_requested),
        percent: progress.min(99),
        detail: detail.to_string(),
        metrics: import_phase_metrics_from_profile(detail),
        partial_results: partial_results_from_profile(data_source_id, detail),
        cancellable: progress < 99,
        cancel_requested,
    }
}

fn import_phase_from_profile(detail: &str, progress: u32) -> ImportPhaseDto {
    match profile_value(detail, "phase").as_deref() {
        Some("attach") => ImportPhaseDto::Attach,
        Some("probe") | Some("probe-resume") | Some("reader-build") => ImportPhaseDto::Probe,
        Some("enumeration") => ImportPhaseDto::Enumerate,
        Some("enum-merge") => ImportPhaseDto::MergeEnumeration,
        Some("analysis-start") | Some("analysis") => ImportPhaseDto::Analyze,
        Some("analysis-merge") => ImportPhaseDto::MergeAnalysis,
        Some("timeline")
        | Some("checkpoint")
        | Some("post-import")
        | Some("post-import-skip")
        | Some("total") => ImportPhaseDto::Finalize,
        _ if progress < 25 => ImportPhaseDto::Attach,
        _ if progress < 70 => ImportPhaseDto::Enumerate,
        _ if progress < 84 => ImportPhaseDto::Analyze,
        _ if progress < 95 => ImportPhaseDto::MergeAnalysis,
        _ => ImportPhaseDto::Finalize,
    }
}

fn import_phase_state_from_profile(detail: &str, cancel_requested: bool) -> ImportPhaseStateDto {
    if cancel_requested {
        return ImportPhaseStateDto::Cancelling;
    }
    let lower = detail.to_ascii_lowercase();
    if lower.contains("cancel") {
        ImportPhaseStateDto::Cancelling
    } else if lower.contains("skipped")
        || profile_value(detail, "phase").as_deref() == Some("post-import-skip")
    {
        ImportPhaseStateDto::Skipped
    } else if lower.contains("complete")
        || lower.contains("ready")
        || lower.contains("already merged")
    {
        ImportPhaseStateDto::Completed
    } else if lower.contains("failed") || lower.contains("hard limit exceeded") {
        ImportPhaseStateDto::Failed
    } else {
        ImportPhaseStateDto::Running
    }
}

fn import_phase_metrics_from_profile(detail: &str) -> ImportPhaseMetricsDto {
    let (rows_processed, rows_total) = rows_from_profile(detail);
    let bytes_processed = profile_u64(detail, "bytes")
        .or_else(|| profile_u64(detail, "dataMb").map(|mb| mb.saturating_mul(1024 * 1024)))
        .unwrap_or(0);
    ImportPhaseMetricsDto {
        elapsed_ms: profile_u64(detail, "elapsedMs").unwrap_or(0),
        rss_mb: profile_u64(detail, "rssMb").unwrap_or(0),
        workers: profile_u64(detail, "workers")
            .or_else(|| profile_nonzero_u64(detail, "activeWorkers"))
            .or_else(|| profile_nonzero_u64(detail, "active"))
            .or_else(|| profile_u64(detail, "workerBudget"))
            .or_else(|| profile_u64(detail, "activeWorkers"))
            .or_else(|| profile_u64(detail, "active"))
            .unwrap_or(0) as u32,
        rows_processed,
        rows_total,
        rows_per_sec: profile_f64(detail, "rowsPerSec"),
        bytes_processed,
        bytes_total: profile_u64(detail, "bytesTotal"),
        mb_per_sec: profile_f64(detail, "mbPerSec"),
        warnings: profile_u64(detail, "warnings").unwrap_or(0) as u32,
        skipped: profile_u64(detail, "skipped").unwrap_or(0) as u32,
        failed: profile_u64(detail, "failed")
            .or_else(|| profile_u64(detail, "failures"))
            .unwrap_or(0) as u32,
    }
}
