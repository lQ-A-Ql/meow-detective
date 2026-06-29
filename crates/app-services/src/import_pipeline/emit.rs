use tauri::{AppHandle, Emitter};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};
use transport::events::{
    EventEnvelope, EventTopic, TOPIC_CACHE_INDEX_STATUS, TOPIC_DATA_SOURCE_IMPORTED,
    TOPIC_IMPORT_PARTIAL_RESULT, TOPIC_IMPORT_PHASE_PROGRESS, TOPIC_JOB_CANCELLATION,
    TOPIC_JOB_PROGRESS, TOPIC_PARTITION_PROGRESS, TOPIC_SEARCH_INDEX_PROGRESS,
    TOPIC_TIMELINE_UPDATED,
};

fn envelope<T: serde::Serialize>(topic: EventTopic, payload: T) -> EventEnvelope<T> {
    EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic,
        ts: chrono::Utc::now(),
        payload,
    }
}

fn emit_event<T: serde::Serialize + Clone>(
    app: &AppHandle,
    topic: &str,
    event: &EventEnvelope<T>,
) -> tauri::Result<()> {
    app.emit_to("main", topic, event)
}

pub(crate) fn emit_job_progress(app: &AppHandle, job_id: &str, progress: u32, detail: &str) {
    let envelope = envelope(
        EventTopic::JobProgress,
        serde_json::json!({
            "jobId": job_id,
            "progress": progress,
            "detail": detail,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_PROGRESS, &envelope) {
        tracing::warn!("Failed to emit job progress event for {}: {}", job_id, e);
    }
}

pub(crate) fn emit_partition_progress(
    app: &AppHandle,
    job_id: &str,
    current_partition: &str,
    completed: u32,
    total: u32,
    partition_pct: u32,
) {
    let envelope = envelope(
        EventTopic::PartitionProgress,
        serde_json::json!({
            "jobId": job_id,
            "currentPartition": current_partition,
            "completedPartitions": completed,
            "totalPartitions": total,
            "partitionProgress": partition_pct,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_PARTITION_PROGRESS, &envelope) {
        tracing::warn!(
            "Failed to emit partition progress event for job {}: {}",
            job_id,
            e
        );
    }
}

pub(crate) fn emit_timeline_updated(app: &AppHandle, event_count: u64) {
    let envelope = envelope(
        EventTopic::TimelineUpdated,
        serde_json::json!({
            "eventCount": event_count,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_TIMELINE_UPDATED, &envelope) {
        tracing::warn!("Failed to emit timeline updated event: {}", e);
    }
}

pub(crate) fn emit_search_index_progress(app: &AppHandle, progress: u32, detail: &str) {
    let envelope = envelope(
        EventTopic::SearchIndexProgress,
        serde_json::json!({
            "progress": progress,
            "detail": detail,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_SEARCH_INDEX_PROGRESS, &envelope) {
        tracing::warn!("Failed to emit search index progress event: {}", e);
    }
}

pub(crate) fn emit_data_source_imported(
    app: &AppHandle,
    case_id: &str,
    data_source: &DataSourceSummaryDto,
    job_id: &str,
) {
    let envelope = envelope(
        EventTopic::DataSourceImported,
        serde_json::json!({
            "caseId": case_id,
            "dataSourceId": data_source.id,
            "name": data_source.name,
            "kind": data_source.kind,
            "jobId": job_id,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_DATA_SOURCE_IMPORTED, &envelope) {
        tracing::warn!(
            "Failed to emit data source imported event for {}: {}",
            data_source.id,
            e
        );
    }
}

pub(crate) fn emit_import_phase_progress(app: &AppHandle, progress: &ImportPhaseProgressDto) {
    let envelope = envelope(EventTopic::ImportPhaseProgress, progress.clone());
    if let Err(e) = emit_event(app, TOPIC_IMPORT_PHASE_PROGRESS, &envelope) {
        tracing::warn!(
            "Failed to emit import phase progress event for job {}: {}",
            progress.job_id,
            e
        );
    }
}

pub(crate) fn emit_import_partial_result(app: &AppHandle, result: &PartialResultDto) {
    let envelope = envelope(EventTopic::ImportPartialResult, result.clone());
    if let Err(e) = emit_event(app, TOPIC_IMPORT_PARTIAL_RESULT, &envelope) {
        tracing::warn!(
            "Failed to emit import partial result event for {}: {}",
            result.scope_id,
            e
        );
    }
}

pub(crate) fn emit_cache_index_status(app: &AppHandle, status: &IndexCacheStatusDto) {
    let envelope = envelope(EventTopic::CacheIndexStatus, status.clone());
    if let Err(e) = emit_event(app, TOPIC_CACHE_INDEX_STATUS, &envelope) {
        tracing::warn!(
            "Failed to emit cache index status event for {}: {}",
            status.cache_key,
            e
        );
    }
}

pub(crate) fn emit_job_cancellation(app: &AppHandle, cancellation: &JobCancellationDto) {
    let envelope = envelope(EventTopic::JobCancellation, cancellation.clone());
    if let Err(e) = emit_event(app, TOPIC_JOB_CANCELLATION, &envelope) {
        tracing::warn!(
            "Failed to emit job cancellation event for {}: {}",
            cancellation.job_id,
            e
        );
    }
}
