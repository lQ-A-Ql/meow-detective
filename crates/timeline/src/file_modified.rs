use crate::{is_meaningful_timestamp, TimelineEventKind};
use domain::{EntryType, FileEntry, TimelineEvent, TimelineEventId};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn project_file_modified(file: &FileEntry) -> Option<TimelineEvent> {
    if file.entry_type != EntryType::File {
        return None;
    }
    let timestamp = file
        .modified_at
        .filter(|value| is_meaningful_timestamp(*value))?;
    let event_kind = TimelineEventKind::FileModified;
    let attrs = BTreeMap::from([(
        "timestampField".to_string(),
        Value::String("modifiedAt".to_string()),
    )]);
    Some(TimelineEvent {
        id: TimelineEventId(format!("file-modified:{}", file.id.0)),
        source_object_id: file.id.0.clone(),
        event_type: event_kind.to_string(),
        timestamp,
        title: format!("File modified: {}", file.name),
        description: format!("{} modified", file.path),
        parser_id: Some("timeline.file_modified".to_string()),
        parser_version: Some("1".to_string()),
        confidence: Some(1.0),
        source_attribution: Some(event_kind.to_string()),
        attrs,
    })
}
