use tauri::{AppHandle, Emitter};
use transport::events::{
    EventEnvelope, EventTopic, TOPIC_ARTIFACT_ADDED, TOPIC_CASE_CLOSED, TOPIC_CASE_OPENED,
    TOPIC_JOB_COMPLETED, TOPIC_JOB_CREATED, TOPIC_JOB_FAILED, TOPIC_JOB_PROGRESS,
    TOPIC_JOB_STARTED, TOPIC_PARTITION_PROGRESS, TOPIC_SEARCH_INDEX_PROGRESS,
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
