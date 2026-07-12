use transport::dto::{
    IndexCacheStatusDto, PartialResultDto, PartialResultKindDto, ResultFreshnessDto,
};

use super::parsing::{profile_u64, profile_value, rows_from_profile};

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
        Some("enum-merge") => file_results(scope_id, detail, &lower),
        Some("analysis") => analysis_partial_result(scope_id, detail),
        Some("post-import-skip") => deferred_analysis_results(scope_id),
        Some("post-import") => {
            analysis_ready_results(scope_id, post_import_counts_from_profile(detail))
        }
        _ => Vec::new(),
    }
}

fn file_results(scope_id: &str, detail: &str, lower: &str) -> Vec<PartialResultDto> {
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

fn analysis_partial_result(scope_id: &str, detail: &str) -> Vec<PartialResultDto> {
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

fn deferred_analysis_results(scope_id: &str) -> Vec<PartialResultDto> {
    analysis_slice_results(scope_id, 0, None, ResultFreshnessDto::Deferred)
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
    cache_statuses_for_phase(scope_id, detail)
}

fn cache_statuses_for_phase(scope_id: &str, detail: &str) -> Vec<IndexCacheStatusDto> {
    match profile_value(detail, "phase").as_deref() {
        Some("analysis-start") => analysis_cache_statuses(
            scope_id,
            "warming",
            0,
            profile_u64(detail, "pendingTasks"),
            Some("Post-import analysis queued; derived caches warming"),
        ),
        Some("analysis") => warming_analysis_cache_statuses(scope_id, detail),
        Some("post-import-skip") => analysis_cache_statuses(
            scope_id,
            "deferred",
            0,
            None,
            Some("Metadata-only import deferred timeline, artifact, and search index caches"),
        ),
        Some("post-import") => {
            analysis_cache_ready_statuses(scope_id, post_import_counts_from_profile(detail))
        }
        _ => Vec::new(),
    }
}

fn warming_analysis_cache_statuses(scope_id: &str, detail: &str) -> Vec<IndexCacheStatusDto> {
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
    ["timeline:events", "artifacts:family", "search:index"]
        .into_iter()
        .map(|key| cache_status(key, scope_id, state, indexed_count, total_count, message))
        .collect()
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
    [
        (PartialResultKindDto::TimelineEvents, "timeline:events"),
        (PartialResultKindDto::ArtifactFamily, "artifacts:family"),
        (PartialResultKindDto::SearchIndex, "search:index"),
    ]
    .into_iter()
    .map(|(kind, key)| {
        partial_result(
            kind,
            scope_id,
            ready_count,
            total_estimate,
            key,
            freshness.clone(),
        )
    })
    .collect()
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
