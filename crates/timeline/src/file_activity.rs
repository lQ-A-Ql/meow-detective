use crate::{is_meaningful_timestamp, TimelineEventKind};
use chrono::{DateTime, Utc};
use domain::{EntryType, FileEntry, TimelineEvent, TimelineEventId};
use serde_json::Value;
use std::collections::BTreeMap;

struct FileActivitySpec {
    kind: TimelineEventKind,
    id_prefix: &'static str,
    timestamp_field: &'static str,
    title_prefix: &'static str,
    description_suffix: &'static str,
    parser_id: &'static str,
    timestamp_semantics: &'static str,
    confidence: f32,
}

pub fn project_file_activity(file: &FileEntry) -> Vec<TimelineEvent> {
    if file.entry_type != EntryType::File {
        return Vec::new();
    }

    let mut events = Vec::with_capacity(4);
    push_event(
        &mut events,
        file,
        file.created_at,
        FileActivitySpec {
            kind: TimelineEventKind::FileCreated,
            id_prefix: "file-created:",
            timestamp_field: "createdAt",
            title_prefix: "File created: ",
            description_suffix: " created",
            parser_id: "timeline.file_created",
            timestamp_semantics: "filesystem creation or birth timestamp",
            confidence: 1.0,
        },
    );
    push_event(
        &mut events,
        file,
        file.modified_at,
        FileActivitySpec {
            kind: TimelineEventKind::FileModified,
            id_prefix: "file-modified:",
            timestamp_field: "modifiedAt",
            title_prefix: "File modified: ",
            description_suffix: " modified",
            parser_id: "timeline.file_modified",
            timestamp_semantics: "filesystem content modification timestamp",
            confidence: 1.0,
        },
    );
    push_event(
        &mut events,
        file,
        file.accessed_at,
        FileActivitySpec {
            kind: TimelineEventKind::FileAccessed,
            id_prefix: "file-accessed:",
            timestamp_field: "accessedAt",
            title_prefix: "File accessed: ",
            description_suffix: " accessed",
            parser_id: "timeline.file_accessed",
            timestamp_semantics: "filesystem access timestamp; does not prove execution",
            confidence: 1.0,
        },
    );
    if file.deleted {
        push_event(
            &mut events,
            file,
            file.changed_at,
            FileActivitySpec {
                kind: TimelineEventKind::FileDeleted,
                id_prefix: "file-deleted:",
                timestamp_field: "changedAt",
                title_prefix: "Deleted file record: ",
                description_suffix: " is marked deleted",
                parser_id: "timeline.file_deleted",
                timestamp_semantics:
                    "metadata change timestamp on a deleted record; deletion time is approximate",
                confidence: 0.65,
            },
        );
    }
    events
}

fn push_event(
    events: &mut Vec<TimelineEvent>,
    file: &FileEntry,
    timestamp: Option<DateTime<Utc>>,
    spec: FileActivitySpec,
) {
    let Some(timestamp) = timestamp.filter(|value| is_meaningful_timestamp(*value)) else {
        return;
    };
    let attrs = BTreeMap::from([
        (
            "timestampField".to_string(),
            Value::String(spec.timestamp_field.to_string()),
        ),
        (
            "timestampSemantics".to_string(),
            Value::String(spec.timestamp_semantics.to_string()),
        ),
    ]);
    events.push(TimelineEvent {
        id: TimelineEventId(format!("{}{}", spec.id_prefix, file.id.0)),
        source_object_id: file.id.0.clone(),
        event_type: spec.kind.to_string(),
        timestamp,
        title: format!("{}{}", spec.title_prefix, file.name),
        description: format!("{}{}", file.path, spec.description_suffix),
        parser_id: Some(spec.parser_id.to_string()),
        parser_version: Some("2".to_string()),
        confidence: Some(spec.confidence),
        source_attribution: Some(spec.kind.to_string()),
        attrs,
    });
}
