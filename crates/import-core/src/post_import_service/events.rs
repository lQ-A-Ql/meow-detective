use super::{ApplicationEventSink, CapabilityWork};
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow;
use transport::events::{EventEnvelope, EventTopic};

pub fn emit(
    sink: Option<&dyn ApplicationEventSink>,
    topic: EventTopic,
    payload: serde_json::Value,
) -> std::io::Result<()> {
    if let Some(sink) = sink {
        sink.emit(&EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            topic,
            ts: chrono::Utc::now(),
            payload,
        })?;
    }
    Ok(())
}

pub(super) fn status(
    sink: Option<&dyn ApplicationEventSink>,
    row: &SourceCapabilityRow,
) -> std::io::Result<()> {
    emit(
        sink,
        EventTopic::CapabilityStatusChanged,
        serde_json::json!({
            "caseId": row.case_id, "dataSourceId": row.data_source_id, "capability": row.capability,
            "state": row.state, "revision": row.revision,
        }),
    )
}

pub(super) fn progress(
    work: &CapabilityWork,
    done: u64,
    total: u64,
    phase: &str,
) -> std::io::Result<()> {
    emit(
        work.events.as_deref(),
        EventTopic::CapabilityProgress,
        serde_json::json!({
            "caseId": work.row.case_id, "dataSourceId": work.row.data_source_id,
            "capability": work.row.capability, "done": done, "total": total, "phase": phase,
        }),
    )
}

pub(super) fn timeline_updated(work: &CapabilityWork, count: u64) -> std::io::Result<()> {
    emit(
        work.events.as_deref(),
        EventTopic::TimelineUpdated,
        serde_json::json!({
            "caseId": work.row.case_id, "dataSourceId": work.row.data_source_id, "eventCount": count,
        }),
    )
}
