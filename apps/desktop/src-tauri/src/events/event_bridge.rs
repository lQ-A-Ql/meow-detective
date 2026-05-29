use tauri::{AppHandle, Emitter};
use transport::events::{
    EventEnvelope, TOPIC_JOB_COMPLETED, TOPIC_JOB_FAILED, TOPIC_JOB_PROGRESS,
    TOPIC_PARTITION_PROGRESS,
};

pub fn emit_event<T: serde::Serialize + Clone>(
    app: &AppHandle,
    topic: &str,
    event: &EventEnvelope<T>,
) -> tauri::Result<()> {
    app.emit(topic, event)
}

pub fn emit_job_progress(app: &AppHandle, job_id: &str, progress: u32, detail: &str) {
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: TOPIC_JOB_PROGRESS.to_string(),
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "jobId": job_id,
            "progress": progress,
            "detail": detail,
        }),
    };
    if let Err(e) = emit_event(app, TOPIC_JOB_PROGRESS, &envelope) {
        tracing::warn!("Failed to emit job progress event for {}: {}", job_id, e);
    }
}

pub fn emit_job_completed(app: &AppHandle, job_id: &str, message: &str) {
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: TOPIC_JOB_COMPLETED.to_string(),
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "jobId": job_id,
            "message": message,
        }),
    };
    if let Err(e) = emit_event(app, TOPIC_JOB_COMPLETED, &envelope) {
        tracing::warn!("Failed to emit job completed event for {}: {}", job_id, e);
    }
}

pub fn emit_job_failed(app: &AppHandle, job_id: &str, error: &str) {
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: TOPIC_JOB_FAILED.to_string(),
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "jobId": job_id,
            "error": error,
        }),
    };
    if let Err(e) = emit_event(app, TOPIC_JOB_FAILED, &envelope) {
        tracing::warn!(
            "Failed to emit job failed event for {}: {} (original error: {})",
            job_id,
            e,
            error
        );
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
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: TOPIC_PARTITION_PROGRESS.to_string(),
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "jobId": job_id,
            "currentPartition": current_partition,
            "completedPartitions": completed,
            "totalPartitions": total,
            "partitionProgress": partition_pct,
        }),
    };
    if let Err(e) = emit_event(app, TOPIC_PARTITION_PROGRESS, &envelope) {
        tracing::warn!(
            "Failed to emit partition progress event for job {}: {}",
            job_id,
            e
        );
    }
}
