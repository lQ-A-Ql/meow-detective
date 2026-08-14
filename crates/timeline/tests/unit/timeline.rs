use super::*;
use chrono::{TimeZone, Utc};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent, TimelineEventId};
use std::collections::BTreeMap;

fn file(modified_at: Option<chrono::DateTime<Utc>>) -> FileEntry {
    FileEntry {
        id: FileEntryId("file-1".to_string()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".to_string()),
        path: "Users/investigator/evidence.txt".to_string(),
        name: "evidence.txt".to_string(),
        entry_type: EntryType::File,
        size: Some(42),
        ext: Some("txt".to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        created_at: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).single(),
        modified_at,
        accessed_at: Utc.with_ymd_and_hms(2024, 1, 3, 1, 0, 0).single(),
        changed_at: Utc.with_ymd_and_hms(2024, 1, 4, 1, 0, 0).single(),
        hash_sha256: None,
    }
}

#[test]
fn file_projection_emits_created_modified_and_accessed_timestamps() {
    let modified_at = Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).single();
    let events = project_file_activity(&file(modified_at));

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event_type, "FILE_CREATED");
    assert_eq!(events[1].id.0, "file-modified:file-1");
    assert_eq!(events[1].event_type, "FILE_MODIFIED");
    assert_eq!(events[1].timestamp, modified_at.expect("timestamp"));
    assert_eq!(events[2].event_type, "FILE_ACCESSED");
    assert_eq!(
        events[2].attrs["timestampSemantics"],
        "filesystem access timestamp; does not prove execution"
    );
}

#[test]
fn file_projection_rejects_missing_epoch_and_directory_timestamps() {
    let mut without_timestamps = file(None);
    without_timestamps.created_at = None;
    without_timestamps.accessed_at = None;
    without_timestamps.changed_at = None;
    assert!(project_file_activity(&without_timestamps).is_empty());

    let mut epoch_only = without_timestamps.clone();
    epoch_only.modified_at = Some(chrono::DateTime::UNIX_EPOCH);
    assert!(project_file_activity(&epoch_only).is_empty());

    let mut directory = file(Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).single());
    directory.entry_type = EntryType::Directory;
    assert!(project_file_activity(&directory).is_empty());
}

#[test]
fn deleted_file_projection_uses_changed_timestamp_with_qualified_confidence() {
    let mut deleted = file(None);
    deleted.created_at = None;
    deleted.accessed_at = None;
    deleted.deleted = true;

    let events = project_file_activity(&deleted);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "FILE_DELETED");
    assert_eq!(
        events[0].parser_id.as_deref(),
        Some("timeline.file_deleted")
    );
    assert_eq!(events[0].confidence, Some(0.65));
    assert!(events[0].attrs["timestampSemantics"]
        .as_str()
        .expect("timestamp semantics")
        .contains("approximate"));
}

#[test]
fn event_kind_allowlist_is_explicit() {
    assert_eq!(TimelineEventKind::ALL.len(), 9);
    assert_eq!(
        TimelineEventKind::parse("REGISTRY_SAM_LAST_LOGIN"),
        Some(TimelineEventKind::RegistrySamLastLogin)
    );
    assert!(TimelineEventKind::RegistrySamLastLogin.is_registry());
    assert!(!TimelineEventKind::FileModified.is_registry());
    assert!(!TimelineEventKind::FileDeleted.is_registry());
    assert!(TimelineEventKind::FileExecuted.is_analysis_event());
    assert!(TimelineEventKind::parse("REGISTRY_USER_ASSIST_LAST_RUN").is_none());
    assert!(TimelineEventKind::parse("EVTX_EVENT").is_none());
    assert!(TimelineEventKind::parse("REGISTRY_UNKNOWN").is_none());
}

#[test]
fn unsupported_events_are_removed() {
    let timestamp = Utc
        .with_ymd_and_hms(2024, 1, 2, 1, 0, 0)
        .single()
        .expect("timestamp");
    let event = |id: &str, event_type: &str| TimelineEvent {
        id: TimelineEventId(id.to_string()),
        source_object_id: "file-1".to_string(),
        event_type: event_type.to_string(),
        timestamp,
        title: id.to_string(),
        description: String::new(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    };
    let mut events = vec![
        event("registry", "REGISTRY_HIVE_LAST_WRITE"),
        event("file", "FILE_MODIFIED"),
        event("evtx", "EVTX_EVENT"),
    ];

    retain_supported_events(&mut events);

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id.0, "registry");
    assert_eq!(events[1].id.0, "file");
}
