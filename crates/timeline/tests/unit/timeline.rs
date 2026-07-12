use super::*;
use chrono::{TimeZone, Utc};
use domain::{DataSourceId, EntryType};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn ts(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

fn ts_hms(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
        .unwrap()
}

/// Full-control builder so every test sets exactly what it needs.
#[allow(clippy::too_many_arguments)]
fn make_entry(
    id: &str,
    name: &str,
    path: &str,
    entry_type: EntryType,
    deleted: bool,
    created_at: Option<DateTime<Utc>>,
    modified_at: Option<DateTime<Utc>>,
    accessed_at: Option<DateTime<Utc>>,
    changed_at: Option<DateTime<Utc>>,
) -> FileEntry {
    let is_dir = entry_type == EntryType::Directory;
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds-1".to_string()),
        path: path.to_string(),
        name: name.to_string(),
        entry_type,
        size: if is_dir { None } else { Some(1024) },
        ext: if is_dir {
            None
        } else {
            Some("txt".to_string())
        },
        deleted,
        hidden: false,
        system: false,
        encrypted: false,
        created_at,
        modified_at,
        accessed_at,
        changed_at,
        hash_sha256: None,
    }
}

// ---------------------------------------------------------------------------
// 1 – single file, all 4 MACB timestamps present
// ---------------------------------------------------------------------------
#[test]
fn test_project_single_file_macb() {
    let file = make_entry(
        "id-1",
        "doc.txt",
        "/docs/doc.txt",
        EntryType::File,
        false,
        Some(ts(2024, 1, 15)),
        Some(ts(2024, 2, 20)),
        Some(ts(2024, 3, 10)),
        Some(ts(2024, 4, 5)),
    );
    let events = project_file_macb(&file);
    assert_eq!(events.len(), 4, "MACB file should produce 4 events");

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains(&"FILE_CREATED"));
    assert!(types.contains(&"FILE_MODIFIED"));
    assert!(types.contains(&"FILE_ACCESSED"));
    assert!(types.contains(&"FILE_METADATA_CHANGED"));
}

// ---------------------------------------------------------------------------
// 2 – file with no timestamps at all
// ---------------------------------------------------------------------------
#[test]
fn test_project_file_no_timestamps() {
    let file = make_entry(
        "id-2",
        "empty.txt",
        "/empty.txt",
        EntryType::File,
        false,
        None,
        None,
        None,
        None,
    );
    let events = project_file_macb(&file);
    assert_eq!(
        events.len(),
        0,
        "file with no timestamps should produce 0 events"
    );
}

// ---------------------------------------------------------------------------
// 3 – file with only created_at set
// ---------------------------------------------------------------------------
#[test]
fn test_project_file_created_only() {
    let file = make_entry(
        "id-3",
        "new.txt",
        "/tmp/new.txt",
        EntryType::File,
        false,
        Some(ts(2025, 6, 1)),
        None,
        None,
        None,
    );
    let events = project_file_macb(&file);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "FILE_CREATED");
    assert_eq!(events[0].timestamp, ts(2025, 6, 1));
}

// ---------------------------------------------------------------------------
// 4 – deleted file still projects its available MACB timestamps
// ---------------------------------------------------------------------------
#[test]
fn test_project_file_deleted_file() {
    let file = make_entry(
        "id-4",
        "removed.log",
        "/var/log/removed.log",
        EntryType::File,
        true, // deleted = true
        Some(ts(2024, 5, 10)),
        Some(ts(2024, 6, 15)),
        Some(ts(2024, 7, 20)),
        None,
    );
    let events = project_file_macb(&file);
    // Deleted flag does not add a FILE_DELETED event — it simply preserves the
    // available timestamp events the filesystem still records.
    assert_eq!(events.len(), 3, "deleted flag does not alter MACB count");

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        !types.contains(&"FILE_DELETED"),
        "no synthetic FILE_DELETED event"
    );
    assert!(types.contains(&"FILE_CREATED"));
    assert!(types.contains(&"FILE_MODIFIED"));
    assert!(types.contains(&"FILE_ACCESSED"));
}

// ---------------------------------------------------------------------------
// 5 – directory entry (MACB timestamps are projected the same as files)
// ---------------------------------------------------------------------------
#[test]
fn test_project_directory() {
    let dir = make_entry(
        "id-5",
        "Documents",
        "/Users/me/Documents",
        EntryType::Directory,
        false,
        Some(ts(2024, 1, 1)),
        Some(ts(2024, 2, 1)),
        None,
        Some(ts(2024, 3, 1)),
    );
    let events = project_file_macb(&dir);
    // Directories also get MACB events — same projection logic.
    assert_eq!(
        events.len(),
        3,
        "directories project available MACB timestamps"
    );

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains(&"FILE_CREATED"));
    assert!(types.contains(&"FILE_MODIFIED"));
    assert!(types.contains(&"FILE_METADATA_CHANGED"));
}

// ---------------------------------------------------------------------------
// 6 – batch projection of 10 files
// ---------------------------------------------------------------------------
#[test]
fn test_project_multiple_files() {
    let entries: Vec<FileEntry> = (0..10)
        .map(|i| {
            make_entry(
                &format!("id-{}", i),
                &format!("file_{}.txt", i),
                &format!("/data/file_{}.txt", i),
                EntryType::File,
                false,
                Some(ts(2024, 1, 1 + i as u32)), // created  Jan  1-10
                Some(ts(2024, 2, 1 + i as u32)), // modified Feb  1-10
                Some(ts(2024, 3, 1 + i as u32)), // accessed Mar  1-10
                Some(ts(2024, 4, 1 + i as u32)), // changed  Apr  1-10
            )
        })
        .collect();

    let all_events: Vec<TimelineEvent> = entries.iter().flat_map(project_file_macb).collect();

    assert_eq!(all_events.len(), 40, "10 files x 4 MACB timestamps");

    // Verify each source file contributed exactly 4 events.
    for i in 0..10u32 {
        let source_id = format!("id-{}", i);
        let count = all_events
            .iter()
            .filter(|e| e.source_object_id == source_id)
            .count();
        assert_eq!(count, 4, "file {} should have 4 events", i);
    }

    // Spot-check one event from file 0.
    let created_events: Vec<_> = all_events
        .iter()
        .filter(|e| e.source_object_id == "id-0" && e.event_type == "FILE_CREATED")
        .collect();
    assert_eq!(created_events.len(), 1);
    assert_eq!(created_events[0].timestamp, ts(2024, 1, 1));
}

// ---------------------------------------------------------------------------
// 7 – MACB event-type strings are exactly the expected constants
// ---------------------------------------------------------------------------
#[test]
fn test_macb_event_types() {
    let file = make_entry(
        "id-7",
        "types.txt",
        "/types.txt",
        EntryType::File,
        false,
        Some(ts(2024, 6, 1)),
        Some(ts(2024, 6, 2)),
        Some(ts(2024, 6, 3)),
        Some(ts(2024, 6, 4)),
    );
    let events = project_file_macb(&file);

    let expected_types = [
        "FILE_CREATED",
        "FILE_MODIFIED",
        "FILE_ACCESSED",
        "FILE_METADATA_CHANGED",
    ];
    let mut actual_types: Vec<String> = events.iter().map(|e| e.event_type.clone()).collect();

    actual_types.sort();
    let mut expected_sorted: Vec<&str> = expected_types.to_vec();
    expected_sorted.sort();

    assert_eq!(
        actual_types,
        expected_sorted
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );

    // Every event should carry the parser identifier.
    for event in &events {
        assert_eq!(event.parser_id.as_deref(), Some("timeline.macb"));
        assert!(!event.title.is_empty());
        assert!(!event.description.is_empty());
    }
}

// ---------------------------------------------------------------------------
// 8 – events are emitted in chronological order
// ---------------------------------------------------------------------------
#[test]
fn test_timestamp_ordering() {
    let file = make_entry(
        "id-8",
        "ordered.txt",
        "/ordered.txt",
        EntryType::File,
        false,
        Some(ts_hms(2024, 5, 1, 8, 0, 0)),   // created
        Some(ts_hms(2024, 5, 1, 10, 30, 0)), // modified
        Some(ts_hms(2024, 5, 1, 12, 0, 0)),  // accessed
        Some(ts_hms(2024, 5, 1, 14, 15, 0)), // changed
    );
    let events = project_file_macb(&file);

    // With chronologically-ascending timestamps the MACB order is also
    // chronological.
    for w in events.windows(2) {
        assert!(
            w[0].timestamp <= w[1].timestamp,
            "events should be non-decreasing: {:?} > {:?}",
            w[0].timestamp,
            w[1].timestamp,
        );
    }
}

// ---------------------------------------------------------------------------
// 9 – Unix-epoch timestamps (sentinel / "zero") are filtered out
// ---------------------------------------------------------------------------
#[test]
fn test_zero_timestamp_filtered() {
    // The Unix epoch is commonly used as a placeholder for absent timestamps.
    let epoch = DateTime::UNIX_EPOCH;

    let file = make_entry(
        "id-9",
        "epoch.txt",
        "/epoch.txt",
        EntryType::File,
        false,
        Some(epoch),          // created  → filtered
        Some(ts(2024, 5, 1)), // modified → kept
        Some(epoch),          // accessed → filtered
        Some(ts(2024, 6, 1)), // changed  → kept
    );
    let events = project_file_macb(&file);
    assert_eq!(events.len(), 2, "epoch timestamps should be filtered");

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(!types.contains(&"FILE_CREATED"));
    assert!(types.contains(&"FILE_MODIFIED"));
    assert!(!types.contains(&"FILE_ACCESSED"));
    assert!(types.contains(&"FILE_METADATA_CHANGED"));

    // Verify the kept timestamps are the non-epoch ones.
    for event in &events {
        assert_ne!(event.timestamp, epoch);
    }
}

// ---------------------------------------------------------------------------
// 10 – projection is deterministic (same input → same output, excluding ids)
// ---------------------------------------------------------------------------
#[test]
fn test_projection_deterministic() {
    let mk = || {
        make_entry(
            "id-10",
            "det.txt",
            "/det/det.txt",
            EntryType::File,
            false,
            Some(ts(2024, 3, 15)),
            Some(ts(2024, 4, 20)),
            Some(ts(2024, 5, 25)),
            Some(ts(2024, 6, 30)),
        )
    };

    let run1 = project_file_macb(&mk());
    let run2 = project_file_macb(&mk());

    assert_eq!(run1.len(), run2.len());
    for (a, b) in run1.iter().zip(run2.iter()) {
        // IDs are random UUIDs — skip them.
        assert_eq!(a.source_object_id, b.source_object_id);
        assert_eq!(a.event_type, b.event_type);
        assert_eq!(a.timestamp, b.timestamp);
        assert_eq!(a.title, b.title);
        assert_eq!(a.description, b.description);
        assert_eq!(a.parser_id, b.parser_id);
        assert_eq!(a.parser_version, b.parser_version);
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(a.source_attribution, b.source_attribution);
        assert_eq!(a.attrs, b.attrs);
        // id is intentionally excluded — it varies per invocation.
    }
}

// ---------------------------------------------------------------------------
// 11 – file with a future timestamp (ts > now)
// ---------------------------------------------------------------------------
#[test]
fn test_project_file_with_future_timestamp() {
    let file = make_entry(
        "id-11",
        "future.txt",
        "/future/future.txt",
        EntryType::File,
        false,
        Some(ts(2030, 1, 1)),
        None,
        None,
        None,
    );
    let events = project_file_macb(&file);
    assert_eq!(
        events.len(),
        1,
        "should produce 1 event for the future timestamp"
    );
    assert_eq!(events[0].event_type, "FILE_CREATED");
    assert_eq!(events[0].timestamp, ts(2030, 1, 1));
    assert!(
        events[0].timestamp > Utc::now(),
        "timestamp should be in the future"
    );
}

// ---------------------------------------------------------------------------
// 12 – batch projection of 25 files (100 events)
// ---------------------------------------------------------------------------
#[test]
fn test_project_batch_25_files() {
    let entries: Vec<FileEntry> = (0..25)
        .map(|i| {
            make_entry(
                &format!("id-batch-{}", i),
                &format!("file_{}.dat", i),
                &format!("/batch/file_{}.dat", i),
                EntryType::File,
                false,
                Some(ts(2024, 1, 1 + (i % 28) as u32)),
                Some(ts(2024, 2, 1 + (i % 28) as u32)),
                Some(ts(2024, 3, 1 + (i % 28) as u32)),
                Some(ts(2024, 4, 1 + (i % 28) as u32)),
            )
        })
        .collect();

    let all_events: Vec<TimelineEvent> = entries.iter().flat_map(project_file_macb).collect();
    assert_eq!(all_events.len(), 100, "25 files x 4 MACB = 100 events");
}

// ---------------------------------------------------------------------------
// 13 – file with only created_at and accessed_at set (2 events)
// ---------------------------------------------------------------------------
#[test]
fn test_project_file_mixed_timestamps() {
    let file = make_entry(
        "id-13",
        "mixed.txt",
        "/mixed/mixed.txt",
        EntryType::File,
        false,
        Some(ts(2024, 7, 1)), // created
        None,
        Some(ts(2024, 8, 15)), // accessed
        None,
    );
    let events = project_file_macb(&file);
    assert_eq!(events.len(), 2, "only created and accessed set → 2 events");

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains(&"FILE_CREATED"));
    assert!(types.contains(&"FILE_ACCESSED"));
    assert!(!types.contains(&"FILE_MODIFIED"));
    assert!(!types.contains(&"FILE_METADATA_CHANGED"));
}

// ---------------------------------------------------------------------------
// 14 – projecting zero files returns 0 events
// ---------------------------------------------------------------------------
#[test]
fn test_empty_slice_no_events() {
    let entries: Vec<FileEntry> = vec![];
    let events: Vec<TimelineEvent> = entries.iter().flat_map(project_file_macb).collect();
    assert_eq!(events.len(), 0, "empty slice should produce 0 events");
}

// ---------------------------------------------------------------------------
// 15 – event.source_object_id matches file.id.0
// ---------------------------------------------------------------------------
#[test]
fn test_event_source_object_id_matches() {
    let file = make_entry(
        "my-id",
        "source.txt",
        "/source/source.txt",
        EntryType::File,
        false,
        Some(ts(2024, 9, 10)),
        Some(ts(2024, 10, 20)),
        Some(ts(2024, 11, 30)),
        Some(ts(2024, 12, 31)),
    );
    let events = project_file_macb(&file);
    assert_eq!(events.len(), 4);

    for event in &events {
        assert_eq!(
            event.source_object_id, file.id.0,
            "source_object_id should match the file entry id"
        );
    }
}
