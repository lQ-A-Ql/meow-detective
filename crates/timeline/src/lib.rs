use chrono::{DateTime, Utc};
use domain::{FileEntry, FileEntryId, TimelineEvent, TimelineEventId};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn project_file_macb(file: &FileEntry) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    if let Some(ts) = file.created_at {
        events.push(make_event(
            &file.id,
            "FILE_CREATED",
            ts,
            format!("File created: {}", file.name),
            format!("{} created", file.path),
        ));
    }
    if let Some(ts) = file.modified_at {
        events.push(make_event(
            &file.id,
            "FILE_MODIFIED",
            ts,
            format!("File modified: {}", file.name),
            format!("{} modified", file.path),
        ));
    }
    if let Some(ts) = file.accessed_at {
        events.push(make_event(
            &file.id,
            "FILE_ACCESSED",
            ts,
            format!("File accessed: {}", file.name),
            format!("{} accessed", file.path),
        ));
    }
    if let Some(ts) = file.changed_at {
        events.push(make_event(
            &file.id,
            "FILE_METADATA_CHANGED",
            ts,
            format!("File metadata changed: {}", file.name),
            format!("{} metadata changed", file.path),
        ));
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
        attrs: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};
    use domain::{DataSourceId, EntryType};

    fn make_file_entry(name: &str, path: &str) -> FileEntry {
        FileEntry {
            id: FileEntryId("test-id".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: Some("txt".to_string()),
            deleted: false,
            created_at: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            modified_at: Some(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap()),
            accessed_at: Some(Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap()),
            changed_at: Some(Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap()),
            hash_sha256: None,
        }
    }

    #[test]
    fn test_project_file_macb_all_timestamps() {
        let file = make_file_entry("test.txt", "/test/test.txt");
        let events = project_file_macb(&file);
        
        // 应该生成 4 个事件 (MACB)
        assert_eq!(events.len(), 4);
        
        // 验证事件类型
        let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains(&"FILE_CREATED"));
        assert!(event_types.contains(&"FILE_MODIFIED"));
        assert!(event_types.contains(&"FILE_ACCESSED"));
        assert!(event_types.contains(&"FILE_METADATA_CHANGED"));
    }

    #[test]
    fn test_project_file_macb_no_timestamps() {
        let mut file = make_file_entry("test.txt", "/test/test.txt");
        file.created_at = None;
        file.modified_at = None;
        file.accessed_at = None;
        file.changed_at = None;
        
        let events = project_file_macb(&file);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_project_file_macb_partial_timestamps() {
        let mut file = make_file_entry("test.txt", "/test/test.txt");
        file.created_at = None;
        file.changed_at = None;
        
        let events = project_file_macb(&file);
        assert_eq!(events.len(), 2); // only modified and accessed
    }

    #[test]
    fn test_event_source_id() {
        let file = make_file_entry("test.txt", "/test/test.txt");
        let events = project_file_macb(&file);
        
        for event in &events {
            assert_eq!(event.source_object_id, "test-id");
        }
    }

    #[test]
    fn test_event_timestamps() {
        let file = make_file_entry("test.txt", "/test/test.txt");
        let events = project_file_macb(&file);
        
        for event in &events {
            // 验证时间戳在合理范围内
            assert!(event.timestamp.year() >= 2024);
        }
    }

    #[test]
    fn test_directory_no_events() {
        let mut file = make_file_entry("test_dir", "/test");
        file.entry_type = EntryType::Directory;
        
        // 目录也应该生成事件
        let events = project_file_macb(&file);
        assert_eq!(events.len(), 4); // MACB
    }
}
