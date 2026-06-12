//! Typed import progress/profile event mapping.

use std::time::Duration;
use tauri::AppHandle;
use transport::dto::{
    ImportPhaseDto, ImportPhaseMetricsDto, ImportPhaseProgressDto, ImportPhaseStateDto,
    IndexCacheStatusDto, PartialResultDto, PartialResultKindDto, ResultFreshnessDto,
};

use crate::events::event_bridge;

pub(crate) fn emit_phase_profile(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: String,
    cancel_requested: bool,
) {
    emit_import_profile_progress(
        app,
        job_id,
        case_id,
        data_source_id,
        progress,
        &detail,
        cancel_requested,
    );
}

pub(crate) fn emit_import_profile_progress(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: &str,
    cancel_requested: bool,
) {
    tracing::info!("Import profile for {}: {}", job_id.0, detail);
    #[cfg(test)]
    eprintln!("[import-profile] {}% {}", progress.min(99), detail);
    if let Some(app) = app {
        let phase_progress = import_phase_progress_from_profile(
            job_id,
            case_id,
            data_source_id,
            progress,
            detail,
            cancel_requested,
        );
        event_bridge::emit_import_phase_progress(app, &phase_progress);
        for result in &phase_progress.partial_results {
            event_bridge::emit_import_partial_result(app, result);
        }
        for status in cache_statuses_from_profile(data_source_id, detail) {
            event_bridge::emit_cache_index_status(app, &status);
        }
        event_bridge::emit_job_progress(app, &job_id.0, progress.min(99), detail);
    }
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PostImportResultCounts {
    pub(crate) timeline_events: u64,
    pub(crate) artifact_count: u64,
    pub(crate) indexed_count: u64,
}

pub(crate) fn partial_results_from_profile(
    data_source_id: Option<&domain::DataSourceId>,
    detail: &str,
) -> Vec<PartialResultDto> {
    let Some(scope_id) = data_source_id.map(|id| id.0.as_str()) else {
        return Vec::new();
    };
    let lower = detail.to_ascii_lowercase();
    if lower.contains("layout changed") && lower.contains("reinitializing") {
        return analysis_slice_results(scope_id, 0, None, ResultFreshnessDto::Invalidated);
    }
    if lower.contains("already merged") {
        return analysis_slice_results(scope_id, 0, None, ResultFreshnessDto::Stale);
    }

    match profile_value(detail, "phase").as_deref() {
        Some("enum-merge") => {
            let rows = profile_u64(detail, "rows").unwrap_or(0);
            let freshness = if lower.contains("complete") || lower.contains("ready") {
                ResultFreshnessDto::Ready
            } else {
                ResultFreshnessDto::Partial
            };
            vec![
                partial_result(
                    PartialResultKindDto::FileRows,
                    scope_id,
                    rows,
                    Some(rows),
                    "files:rows",
                    freshness.clone(),
                ),
                partial_result(
                    PartialResultKindDto::FileTree,
                    scope_id,
                    rows,
                    Some(rows),
                    "files:tree",
                    freshness,
                ),
            ]
        }
        Some("analysis") => {
            let indexed = profile_u64(detail, "indexed").unwrap_or(0);
            let total = profile_u64(detail, "files")
                .or_else(|| rows_from_profile(detail).1)
                .or_else(|| profile_u64(detail, "queuedTasks"));
            vec![partial_result(
                PartialResultKindDto::SearchIndex,
                scope_id,
                indexed,
                total,
                "search:index",
                ResultFreshnessDto::Partial,
            )]
        }
        Some("post-import-skip") => vec![
            partial_result(
                PartialResultKindDto::TimelineEvents,
                scope_id,
                0,
                None,
                "timeline:events",
                ResultFreshnessDto::Deferred,
            ),
            partial_result(
                PartialResultKindDto::ArtifactFamily,
                scope_id,
                0,
                None,
                "artifacts:family",
                ResultFreshnessDto::Deferred,
            ),
            partial_result(
                PartialResultKindDto::SearchIndex,
                scope_id,
                0,
                None,
                "search:index",
                ResultFreshnessDto::Deferred,
            ),
        ],
        Some("post-import") => {
            let counts = post_import_counts_from_profile(detail);
            analysis_ready_results(scope_id, counts)
        }
        _ => Vec::new(),
    }
}

fn analysis_ready_results(scope_id: &str, counts: PostImportResultCounts) -> Vec<PartialResultDto> {
    vec![
        partial_result(
            PartialResultKindDto::TimelineEvents,
            scope_id,
            counts.timeline_events,
            Some(counts.timeline_events),
            "timeline:events",
            ResultFreshnessDto::Ready,
        ),
        partial_result(
            PartialResultKindDto::ArtifactFamily,
            scope_id,
            counts.artifact_count,
            Some(counts.artifact_count),
            "artifacts:family",
            ResultFreshnessDto::Ready,
        ),
        partial_result(
            PartialResultKindDto::SearchIndex,
            scope_id,
            counts.indexed_count,
            Some(counts.indexed_count),
            "search:index",
            ResultFreshnessDto::Ready,
        ),
    ]
}

pub(crate) fn cache_statuses_from_profile(
    data_source_id: Option<&domain::DataSourceId>,
    detail: &str,
) -> Vec<IndexCacheStatusDto> {
    let Some(scope_id) = data_source_id.map(|id| id.0.as_str()) else {
        return Vec::new();
    };
    let lower = detail.to_ascii_lowercase();
    if lower.contains("layout changed") && lower.contains("reinitializing") {
        return analysis_cache_statuses(
            scope_id,
            "invalidated",
            0,
            None,
            Some("Analysis staging layout changed; derived caches invalidated"),
        );
    }
    if lower.contains("already merged") {
        return analysis_cache_statuses(
            scope_id,
            "reused",
            0,
            None,
            Some("Previously merged analysis output reused"),
        );
    }
    if lower.contains("merging analysis staging dbs") {
        return analysis_cache_statuses(
            scope_id,
            "stale",
            0,
            None,
            Some("Worker output is being merged; existing derived caches may be stale"),
        );
    }

    match profile_value(detail, "phase").as_deref() {
        Some("analysis-start") => {
            let total = profile_u64(detail, "pendingTasks");
            analysis_cache_statuses(
                scope_id,
                "warming",
                0,
                total,
                Some("Post-import analysis queued; derived caches warming"),
            )
        }
        Some("analysis") => {
            let indexed = profile_u64(detail, "indexed").unwrap_or(0);
            let total = profile_u64(detail, "files")
                .or_else(|| rows_from_profile(detail).1)
                .or_else(|| profile_u64(detail, "queuedTasks"));
            analysis_cache_statuses(
                scope_id,
                "warming",
                indexed,
                total,
                Some("Post-import analysis running; derived caches warming"),
            )
        }
        Some("post-import-skip") => analysis_cache_statuses(
            scope_id,
            "deferred",
            0,
            None,
            Some("Metadata-only import deferred timeline, artifact, and search index caches"),
        ),
        Some("post-import") => {
            let counts = post_import_counts_from_profile(detail);
            analysis_cache_ready_statuses(scope_id, counts)
        }
        _ => Vec::new(),
    }
}

fn analysis_cache_ready_statuses(
    scope_id: &str,
    counts: PostImportResultCounts,
) -> Vec<IndexCacheStatusDto> {
    vec![
        cache_status(
            "timeline:events",
            scope_id,
            "ready",
            counts.timeline_events,
            Some(counts.timeline_events),
            Some("Timeline projection ready"),
        ),
        cache_status(
            "artifacts:family",
            scope_id,
            "ready",
            counts.artifact_count,
            Some(counts.artifact_count),
            Some("Artifact analysis cache ready"),
        ),
        cache_status(
            "search:index",
            scope_id,
            "ready",
            counts.indexed_count,
            Some(counts.indexed_count),
            Some("Search index ready"),
        ),
    ]
}

fn analysis_cache_statuses(
    scope_id: &str,
    state: &str,
    indexed_count: u64,
    total_count: Option<u64>,
    message: Option<&str>,
) -> Vec<IndexCacheStatusDto> {
    vec![
        cache_status(
            "timeline:events",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
        cache_status(
            "artifacts:family",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
        cache_status(
            "search:index",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
    ]
}

fn cache_status(
    key_prefix: &str,
    scope_id: &str,
    state: &str,
    indexed_count: u64,
    total_count: Option<u64>,
    message: Option<&str>,
) -> IndexCacheStatusDto {
    IndexCacheStatusDto {
        cache_key: format!("{key_prefix}:{scope_id}"),
        state: state.to_string(),
        indexed_count,
        total_count,
        updated_at: chrono::Utc::now().to_rfc3339(),
        message: message.map(str::to_string),
    }
}

fn analysis_slice_results(
    scope_id: &str,
    ready_count: u64,
    total_estimate: Option<u64>,
    freshness: ResultFreshnessDto,
) -> Vec<PartialResultDto> {
    vec![
        partial_result(
            PartialResultKindDto::TimelineEvents,
            scope_id,
            ready_count,
            total_estimate,
            "timeline:events",
            freshness.clone(),
        ),
        partial_result(
            PartialResultKindDto::ArtifactFamily,
            scope_id,
            ready_count,
            total_estimate,
            "artifacts:family",
            freshness.clone(),
        ),
        partial_result(
            PartialResultKindDto::SearchIndex,
            scope_id,
            ready_count,
            total_estimate,
            "search:index",
            freshness,
        ),
    ]
}

fn partial_result(
    kind: PartialResultKindDto,
    scope_id: &str,
    ready_count: u64,
    total_estimate: Option<u64>,
    key_prefix: &str,
    freshness: ResultFreshnessDto,
) -> PartialResultDto {
    PartialResultDto {
        kind,
        scope_id: scope_id.to_string(),
        ready_count,
        total_estimate,
        query_key: format!("{key_prefix}:{scope_id}"),
        freshness,
    }
}

fn post_import_counts_from_profile(detail: &str) -> PostImportResultCounts {
    PostImportResultCounts {
        timeline_events: profile_u64(detail, "timeline").unwrap_or(0),
        artifact_count: profile_u64(detail, "artifacts").unwrap_or(0),
        indexed_count: profile_u64(detail, "indexed").unwrap_or(0),
    }
}

pub(crate) fn post_import_counts_from_message(message: &str) -> PostImportResultCounts {
    let normalized = message.replace([':', '.', ','], " ");
    let parts: Vec<&str> = normalized.split_whitespace().collect();
    PostImportResultCounts {
        timeline_events: value_after_label(&parts, "Timeline").unwrap_or(0),
        artifact_count: value_after_label(&parts, "Artifacts").unwrap_or(0),
        indexed_count: value_after_label(&parts, "Index").unwrap_or(0),
    }
}

fn value_after_label(parts: &[&str], label: &str) -> Option<u64> {
    parts.windows(2).find_map(|window| {
        (window[0] == label)
            .then(|| window[1].parse::<u64>().ok())
            .flatten()
    })
}

fn import_phase_from_profile(detail: &str, progress: u32) -> ImportPhaseDto {
    match profile_value(detail, "phase").as_deref() {
        Some("attach") => ImportPhaseDto::Attach,
        Some("probe") | Some("probe-resume") | Some("reader-build") => ImportPhaseDto::Probe,
        Some("enumeration") => ImportPhaseDto::Enumerate,
        Some("enum-merge") => ImportPhaseDto::MergeEnumeration,
        Some("analysis-start") | Some("analysis") => ImportPhaseDto::Analyze,
        Some("analysis-merge") => ImportPhaseDto::MergeAnalysis,
        Some("post-import") | Some("post-import-skip") | Some("total") => ImportPhaseDto::Finalize,
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

fn rows_from_profile(detail: &str) -> (u64, Option<u64>) {
    if let Some(processed) = profile_value(detail, "processed") {
        if let Some((done, total)) = processed.split_once('/') {
            return (done.parse::<u64>().unwrap_or(0), total.parse::<u64>().ok());
        }
        if let Ok(rows) = processed.parse::<u64>() {
            return (rows, profile_u64(detail, "files"));
        }
    }
    let rows = profile_u64(detail, "rows").unwrap_or(0);
    (
        rows,
        profile_u64(detail, "files").or_else(|| profile_u64(detail, "pendingTasks")),
    )
}

fn profile_u64(detail: &str, key: &str) -> Option<u64> {
    profile_value(detail, key).and_then(|value| value.parse::<u64>().ok())
}

fn profile_nonzero_u64(detail: &str, key: &str) -> Option<u64> {
    profile_u64(detail, key).filter(|value| *value > 0)
}

fn profile_f64(detail: &str, key: &str) -> Option<f64> {
    profile_value(detail, key).and_then(|value| value.parse::<f64>().ok())
}

fn profile_value(detail: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .map(|value| value.trim_end_matches([',', ';']).to_string())
    })
}

pub(crate) fn elapsed_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

pub(crate) fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

pub(crate) fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub(crate) fn mb_per_sec(bytes: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        bytes_to_mb(bytes)
    } else {
        ((bytes as f64 / (1024.0 * 1024.0)) / secs).round() as u64
    }
}
