use chrono::{DateTime, Utc};
use domain::{FileEntry, FileEntryId, TimelineEvent, TimelineEventId};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn project_file_macb(file: &FileEntry) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    if let Some(ts) = file.created_at {
        events.push(make_event(&file.id, "FILE_CREATED", ts,
            format!("File created: {}", file.name), format!("{} created", file.path)));
    }
    if let Some(ts) = file.modified_at {
        events.push(make_event(&file.id, "FILE_MODIFIED", ts,
            format!("File modified: {}", file.name), format!("{} modified", file.path)));
    }
    if let Some(ts) = file.accessed_at {
        events.push(make_event(&file.id, "FILE_ACCESSED", ts,
            format!("File accessed: {}", file.name), format!("{} accessed", file.path)));
    }
    if let Some(ts) = file.changed_at {
        events.push(make_event(&file.id, "FILE_METADATA_CHANGED", ts,
            format!("File metadata changed: {}", file.name), format!("{} metadata changed", file.path)));
    }
    events
}

fn make_event(source_id: &FileEntryId, event_type: &str, ts: DateTime<Utc>, title: String, description: String) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(Uuid::new_v4().to_string()),
        source_object_id: source_id.0.clone(),
        event_type: event_type.to_string(),
        timestamp: ts,
        title,
        description,
        attrs: BTreeMap::new(),
    }
}
