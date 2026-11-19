//! Capability row writes and status/progress event emission for the scheduler.

use persistence_sqlite::repositories::source_capability_repo::{
    SourceCapabilityRepo, SourceCapabilityRow,
};
use tauri::AppHandle;
use transport::events::{
    EventEnvelope, EventTopic, TOPIC_CAPABILITY_PROGRESS, TOPIC_CAPABILITY_STATUS_CHANGED,
    TOPIC_TIMELINE_UPDATED,
};

use super::QueuedTask;

/// Write a task-confirmed capability row and emit `capability-status-changed`.
/// Revision carries over and bumps only on a transition into `ready`.
pub(super) fn write_row(
    task: &QueuedTask,
    state: &str,
    detail: Option<&str>,
    error_code: Option<&str>,
) {
    let db_path = task.case_root.join("app.db");
    let conn = match app_services::connection::open_existing_case_db(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = %error, "post-import: cannot write capability row");
            return;
        }
    };
    let repo = SourceCapabilityRepo::new(&conn);
    let wire = task.capability.wire();
    let existing = repo
        .get(&task.case_id, &task.source_id, wire)
        .ok()
        .flatten();
    let revision = match existing.as_ref() {
        Some(row) if state == "ready" && row.state != "ready" => row.revision + 1,
        Some(row) => row.revision,
        None => 0,
    };
    let row = SourceCapabilityRow {
        case_id: task.case_id.clone(),
        data_source_id: task.source_id.clone(),
        capability: wire.to_string(),
        state: state.to_string(),
        revision,
        progress_done: 0,
        progress_total: None,
        phase: None,
        detail: detail.map(str::to_string),
        error_code: error_code.map(str::to_string),
        derived: 0,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(error) = repo.upsert(&row) {
        tracing::warn!(error = %error, "post-import: capability row upsert failed");
    }
    emit_status_changed(
        task.app.as_ref(),
        &task.case_id,
        &task.source_id,
        wire,
        state,
        revision,
    );
}

fn emit_status_changed(
    app: Option<&AppHandle>,
    case_id: &str,
    source_id: &str,
    capability: &str,
    state: &str,
    revision: i64,
) {
    let Some(app) = app else { return };
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: EventTopic::CapabilityStatusChanged,
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "caseId": case_id,
            "dataSourceId": source_id,
            "capability": capability,
            "state": state,
            "revision": revision,
        }),
    };
    if let Err(error) =
        crate::events::event_bridge::emit_event(app, TOPIC_CAPABILITY_STATUS_CHANGED, &envelope)
    {
        tracing::warn!(error = %error, "failed to emit capability-status-changed");
    }
}

pub(super) fn emit_capability_progress(
    app: Option<&AppHandle>,
    case_id: &str,
    source_id: &str,
    capability: &str,
    done: u64,
    total: Option<u64>,
    phase: Option<&str>,
) {
    let Some(app) = app else { return };
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: EventTopic::CapabilityProgress,
        ts: chrono::Utc::now(),
        payload: serde_json::json!({
            "caseId": case_id,
            "dataSourceId": source_id,
            "capability": capability,
            "done": done,
            "total": total,
            "phase": phase,
        }),
    };
    if let Err(error) =
        crate::events::event_bridge::emit_event(app, TOPIC_CAPABILITY_PROGRESS, &envelope)
    {
        tracing::warn!(error = %error, "failed to emit capability-progress");
    }
}

pub(super) fn emit_timeline_updated(app: Option<&AppHandle>, event_count: u64) {
    let Some(app) = app else { return };
    let envelope = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        topic: EventTopic::TimelineUpdated,
        ts: chrono::Utc::now(),
        payload: serde_json::json!({ "eventCount": event_count }),
    };
    if let Err(error) =
        crate::events::event_bridge::emit_event(app, TOPIC_TIMELINE_UPDATED, &envelope)
    {
        tracing::warn!(error = %error, "failed to emit timeline-updated");
    }
}
