use tauri::{AppHandle, Emitter};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto, PerformanceReportDto,
};
use transport::events::{
    EventEnvelope, EventTopic, TOPIC_ARTIFACT_ADDED, TOPIC_CACHE_INDEX_STATUS, TOPIC_CASE_CLOSED,
    TOPIC_CASE_OPENED, TOPIC_DATA_SOURCE_IMPORTED, TOPIC_IMPORT_PARTIAL_RESULT,
    TOPIC_IMPORT_PHASE_PROGRESS, TOPIC_JOB_CANCELLATION, TOPIC_JOB_CANCELLED, TOPIC_JOB_COMPLETED,
    TOPIC_JOB_CREATED, TOPIC_JOB_FAILED, TOPIC_JOB_PROGRESS, TOPIC_JOB_STARTED,
    TOPIC_PARTITION_PROGRESS, TOPIC_PERFORMANCE_REPORT_READY, TOPIC_SEARCH_INDEX_PROGRESS,
    TOPIC_TIMELINE_UPDATED,
};

pub fn emit_event<T: serde::Serialize + Clone>(
    app: &AppHandle,
    topic: &str,
    event: &EventEnvelope<T>,
) -> tauri::Result<()> {
    app.emit_to("main", topic, event)
}

fn envelope<T: serde::Serialize>(topic: EventTopic, payload: T) -> EventEnvelope<T> {
    EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic,
        ts: chrono::Utc::now(),
        payload,
    }
}

pub fn emit_case_opened(app: &AppHandle, case_id: &str, case_name: &str) {
    let envelope = envelope(
        EventTopic::CaseOpened,
        serde_json::json!({
            "caseId": case_id,
            "caseName": case_name,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_CASE_OPENED, &envelope) {
        tracing::warn!("Failed to emit case opened event for {}: {}", case_id, e);
    }
}

pub fn emit_case_closed(app: &AppHandle, case_id: &str) {
    let envelope = envelope(
        EventTopic::CaseClosed,
        serde_json::json!({
            "caseId": case_id,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_CASE_CLOSED, &envelope) {
        tracing::warn!("Failed to emit case closed event for {}: {}", case_id, e);
    }
}

pub fn emit_job_created(app: &AppHandle, job_id: &str, name: &str) {
    let envelope = envelope(
        EventTopic::JobCreated,
        serde_json::json!({
            "jobId": job_id,
            "name": name,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_CREATED, &envelope) {
        tracing::warn!("Failed to emit job created event for {}: {}", job_id, e);
    }
}

pub fn emit_job_started(app: &AppHandle, job_id: &str, detail: &str) {
    let envelope = envelope(
        EventTopic::JobStarted,
        serde_json::json!({
            "jobId": job_id,
            "detail": detail,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_STARTED, &envelope) {
        tracing::warn!("Failed to emit job started event for {}: {}", job_id, e);
    }
}

pub fn emit_job_progress(app: &AppHandle, job_id: &str, progress: u32, detail: &str) {
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

pub fn emit_job_completed(app: &AppHandle, job_id: &str, message: &str) {
    let envelope = envelope(
        EventTopic::JobCompleted,
        serde_json::json!({
            "jobId": job_id,
            "message": message,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_COMPLETED, &envelope) {
        tracing::warn!("Failed to emit job completed event for {}: {}", job_id, e);
    }
}

pub fn emit_job_failed(app: &AppHandle, job_id: &str, error: &str) {
    let envelope = envelope(
        EventTopic::JobFailed,
        serde_json::json!({
            "jobId": job_id,
            "error": error,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_FAILED, &envelope) {
        tracing::warn!(
            "Failed to emit job failed event for {}: {} (original error: {})",
            job_id,
            e,
            error
        );
    }
}

pub fn emit_job_cancelled(app: &AppHandle, job_id: &str, reason: &str) {
    let envelope = envelope(
        EventTopic::JobCancelled,
        serde_json::json!({
            "jobId": job_id,
            "reason": reason,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_JOB_CANCELLED, &envelope) {
        tracing::warn!("Failed to emit job cancelled event for {}: {}", job_id, e);
    }
}

pub fn emit_job_cancellation(app: &AppHandle, cancellation: &JobCancellationDto) {
    let envelope = envelope(EventTopic::JobCancellation, cancellation.clone());
    if let Err(e) = emit_event(app, TOPIC_JOB_CANCELLATION, &envelope) {
        tracing::warn!(
            "Failed to emit job cancellation event for {}: {}",
            cancellation.job_id,
            e
        );
    }
}

pub fn emit_data_source_imported(
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

pub fn emit_artifact_added(app: &AppHandle, artifact_id: &str, artifact_type: &str) {
    let envelope = envelope(
        EventTopic::ArtifactAdded,
        serde_json::json!({
            "artifactId": artifact_id,
            "artifactType": artifact_type,
        }),
    );
    if let Err(e) = emit_event(app, TOPIC_ARTIFACT_ADDED, &envelope) {
        tracing::warn!(
            "Failed to emit artifact added event for {}: {}",
            artifact_id,
            e
        );
    }
}

pub fn emit_timeline_updated(app: &AppHandle, event_count: u64) {
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

pub fn emit_search_index_progress(app: &AppHandle, progress: u32, detail: &str) {
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

pub fn emit_partition_progress(
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

pub fn emit_import_phase_progress(app: &AppHandle, progress: &ImportPhaseProgressDto) {
    let envelope = envelope(EventTopic::ImportPhaseProgress, progress.clone());
    if let Err(e) = emit_event(app, TOPIC_IMPORT_PHASE_PROGRESS, &envelope) {
        tracing::warn!(
            "Failed to emit import phase progress event for job {}: {}",
            progress.job_id,
            e
        );
    }
}

pub fn emit_import_partial_result(app: &AppHandle, result: &PartialResultDto) {
    let envelope = envelope(EventTopic::ImportPartialResult, result.clone());
    if let Err(e) = emit_event(app, TOPIC_IMPORT_PARTIAL_RESULT, &envelope) {
        tracing::warn!(
            "Failed to emit import partial result event for {}: {}",
            result.scope_id,
            e
        );
    }
}

pub fn emit_cache_index_status(app: &AppHandle, status: &IndexCacheStatusDto) {
    let envelope = envelope(EventTopic::CacheIndexStatus, status.clone());
    if let Err(e) = emit_event(app, TOPIC_CACHE_INDEX_STATUS, &envelope) {
        tracing::warn!(
            "Failed to emit cache index status event for {}: {}",
            status.cache_key,
            e
        );
    }
}

pub fn emit_performance_report_ready(app: &AppHandle, report: &PerformanceReportDto) {
    let envelope = envelope(EventTopic::PerformanceReportReady, report.clone());
    if let Err(e) = emit_event(app, TOPIC_PERFORMANCE_REPORT_READY, &envelope) {
        tracing::warn!(
            "Failed to emit performance report ready event for {}: {}",
            report.summary.report_id,
            e
        );
    }
}
