use chrono::{DateTime, Utc};
use domain::{FileEntry, FileEntryId, TimelineEvent, TimelineEventId};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Returns true if the timestamp is the Unix epoch (1970-01-01T00:00:00Z),
/// which filesystems commonly use as a sentinel for "no timestamp".
fn is_epoch(ts: DateTime<Utc>) -> bool {
    ts == DateTime::UNIX_EPOCH
}

pub fn project_file_macb(file: &FileEntry) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    if let Some(ts) = file.created_at {
        if !is_epoch(ts) {
            events.push(make_event(
                &file.id,
                "FILE_CREATED",
                ts,
                format!("File created: {}", file.name),
                format!("{} created", file.path),
            ));
        }
    }
    if let Some(ts) = file.modified_at {
        if !is_epoch(ts) {
            events.push(make_event(
                &file.id,
                "FILE_MODIFIED",
                ts,
                format!("File modified: {}", file.name),
                format!("{} modified", file.path),
            ));
        }
    }
    if let Some(ts) = file.accessed_at {
        if !is_epoch(ts) {
            events.push(make_event(
                &file.id,
                "FILE_ACCESSED",
                ts,
                format!("File accessed: {}", file.name),
                format!("{} accessed", file.path),
            ));
        }
    }
    if let Some(ts) = file.changed_at {
        if !is_epoch(ts) {
            events.push(make_event(
                &file.id,
                "FILE_METADATA_CHANGED",
                ts,
                format!("File metadata changed: {}", file.name),
                format!("{} metadata changed", file.path),
            ));
        }
    }
    events
}

fn make_event(
    source_id: &FileEntryId,
    event_type: &str,
    ts: DateTime<Utc>,
    title: String,
    description: String,
) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(Uuid::new_v4().to_string()),
        source_object_id: source_id.0.clone(),
        event_type: event_type.to_string(),
        timestamp: ts,
        title,
        description,
        parser_id: Some("timeline.macb".to_string()),
        parser_version: None,
        confidence: None,
        source_attribution: Some(event_type.to_string()),
        attrs: BTreeMap::new(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/timeline.rs"]
mod tests;
