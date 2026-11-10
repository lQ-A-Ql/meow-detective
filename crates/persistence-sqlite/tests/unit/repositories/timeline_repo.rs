use super::*;
use domain::TimelineEventId;
use std::collections::BTreeMap;

fn setup_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let conn = crate::open_or_create_source(&path).unwrap();
    crate::source_fjall::configure_source_identity(&conn, "source-1").unwrap();
    crate::source_fjall::open_store_for_write(&conn)
        .unwrap()
        .unwrap();
    (root, conn)
}

fn make_event(id: &str, event_type: &str, ts: &str) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(id.to_string()),
        source_object_id: "src-1".to_string(),
        event_type: event_type.to_string(),
        timestamp: chrono::DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .with_timezone(&chrono::Utc),
        title: format!("Event {}", id),
        description: format!("Desc {}", id),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

fn ids(events: &[TimelineEvent]) -> Vec<String> {
    events.iter().map(|event| event.id.0.clone()).collect()
}

#[test]
fn insert_batch_then_count_returns_correct_number() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    let events = vec![
        make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
        make_event("e2", "file_modify", "2025-01-01T01:00:00Z"),
        make_event("e3", "file_create", "2025-01-01T02:00:00Z"),
    ];
    repo.insert_batch(&events).unwrap();
    assert_eq!(repo.count().unwrap(), 3);
}

#[test]
fn query_returns_events_ordered_by_timestamp() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[
        make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
        make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
        make_event("e3", "file_delete", "2025-01-03T00:00:00Z"),
    ])
    .unwrap();
    assert_eq!(ids(&repo.query(0, 10).unwrap()), ["e3", "e2", "e1"]);
}

#[test]
fn query_filtered_filters_by_event_type() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[
        make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
        make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
        make_event("e3", "file_create", "2025-01-03T00:00:00Z"),
    ])
    .unwrap();
    let results = repo
        .query_filtered(0, 10, None, None, Some("file_create"))
        .unwrap();
    assert_eq!(ids(&results), ["e3", "e1"]);
}

#[test]
fn keyset_pagination_preserves_timestamp_and_id_order_without_duplicates() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[
        make_event("same-04", "file_modify", "2025-01-03T00:00:00Z"),
        make_event("same-02", "file_modify", "2025-01-03T00:00:00Z"),
        make_event("same-03", "file_modify", "2025-01-03T00:00:00Z"),
        make_event("older-02", "file_modify", "2025-01-02T00:00:00Z"),
        make_event("older-01", "file_modify", "2025-01-02T00:00:00Z"),
    ])
    .unwrap();
    let first = repo
        .query_filtered_after(None, 2, None, None, Some("file_modify"))
        .unwrap();
    let second = repo
        .query_filtered_after(
            first.last().map(|row| &row.sort_key),
            2,
            None,
            None,
            Some("file_modify"),
        )
        .unwrap();
    let third = repo
        .query_filtered_after(
            second.last().map(|row| &row.sort_key),
            2,
            None,
            None,
            Some("file_modify"),
        )
        .unwrap();
    assert_eq!(
        first
            .into_iter()
            .chain(second)
            .chain(third)
            .map(|row| row.event.id.0)
            .collect::<Vec<_>>(),
        ["same-02", "same-03", "same-04", "older-01", "older-02"]
    );
}

#[test]
fn snapshot_keyset_rejects_later_writes_through_native_revision() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[
        make_event("latest", "file_modify", "2025-01-03T00:00:00Z"),
        make_event("middle", "file_modify", "2025-01-02T00:00:00Z"),
        make_event("oldest", "file_modify", "2025-01-01T00:00:00Z"),
    ])
    .unwrap();
    let revision = repo.cursor_revision().unwrap();
    let first = repo
        .query_filtered_after(None, 1, None, None, Some("file_modify"))
        .unwrap();
    repo.insert_batch(&[make_event(
        "inserted-after-snapshot",
        "file_modify",
        "2025-01-04T00:00:00Z",
    )])
    .unwrap();
    assert_ne!(repo.cursor_revision().unwrap(), revision);
    assert_eq!(first[0].event.id.0, "latest");
}

#[test]
fn count_filtered_with_time_range() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[
        make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
        make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
        make_event("e3", "file_delete", "2025-01-03T00:00:00Z"),
    ])
    .unwrap();
    assert_eq!(
        repo.count_filtered(
            Some("2025-01-02T00:00:00+00:00"),
            Some("2025-01-02T23:59:59+00:00"),
            None
        )
        .unwrap(),
        1
    );
    assert_eq!(repo.count_filtered(None, None, None).unwrap(), 3);
}

#[test]
fn timeline_provenance_round_trips() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    let mut event = make_event("e1", "file_modify", "2025-01-01T00:00:00Z");
    event.parser_id = Some("timeline.macb".to_string());
    event.parser_version = Some("1.2.3".to_string());
    event.confidence = Some(0.87);
    event.source_attribution = Some("modified_at".to_string());
    repo.insert_batch(&[event]).unwrap();
    let rows = repo.query(0, 10).unwrap();
    assert_eq!(rows[0].parser_id.as_deref(), Some("timeline.macb"));
    assert_eq!(rows[0].parser_version.as_deref(), Some("1.2.3"));
    assert_eq!(rows[0].confidence, Some(0.87));
    assert_eq!(rows[0].source_attribution.as_deref(), Some("modified_at"));
}

#[test]
fn find_by_id_returns_native_record() {
    let (_root, conn) = setup_db();
    let repo = TimelineRepo::new(&conn);
    repo.insert_batch(&[make_event("event", "file_modify", "2025-01-01T00:00:00Z")])
        .unwrap();
    assert_eq!(repo.find_by_id("event").unwrap().unwrap().id.0, "event");
}
