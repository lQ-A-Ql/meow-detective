use app_services::{
    case_service,
    source_db::{open_registered_fjall_source_read_write, open_source_db},
    timeline_service::{query_timeline_filtered_for_case, TimelineQuery},
};
use chrono::{TimeZone, Utc};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{PersistMode, TimelineRecord};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::collections::BTreeMap;

#[path = "support/fjall_projection.rs"]
mod projection_support;

fn event(id: &str, timestamp: chrono::DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: "fjall-source".to_owned(),
        source_object_id: format!("file-{id}"),
        event_type: "FILE_CREATED".to_owned(),
        timestamp,
        title: id.to_owned(),
        description: format!("created {id}"),
        parser_id: Some("test.fjall".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::new(),
    }
}

fn source_event(source_id: &str, id: &str, timestamp: chrono::DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        data_source_id: source_id.to_owned(),
        ..event(id, timestamp)
    }
}

fn publish_source(
    case: &app_services::active_case::ActiveCase,
    source_id: &str,
    records: &[TimelineRecord],
) {
    let source = DataSource {
        id: DataSourceId(source_id.to_owned()),
        name: source_id.to_owned(),
        kind: DataSourceKind::E01,
        source_path: case.case_root.join(format!("{source_id}.E01")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    case.with_conn(|connection| {
        let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
        storage.import_state = "ready".to_owned();
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)
    })
    .unwrap();
    let source_conn = open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    let fjall = open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    for record in records {
        fjall.timeline_upsert(record, PersistMode::Buffer).unwrap();
    }
    fjall.persist(PersistMode::SyncAll).unwrap();
    drop(fjall);
    let store = persistence_source::detail::open_store_for_write(&source_conn)
        .unwrap()
        .unwrap();
    persistence_source::fjall::seal_source_generation(
        &store,
        source_id,
        "fixture",
        &"a".repeat(64),
    )
    .unwrap();
    drop(store);
    drop(source_conn);
    let fjall = open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    projection_support::mark_source_projection_complete(&fjall, source_id);
}

/// Walks the whole result with `next_cursor` and returns the row ids in page
/// order together with the reported total.
fn page_through(
    case: &app_services::active_case::ActiveCase,
    event_type: Option<&str>,
    page_size: u32,
) -> (Vec<String>, u64) {
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let query = TimelineQuery {
            offset: 0,
            limit: page_size,
            time_start: None,
            time_end: None,
            event_type,
            cursor: cursor.as_deref(),
        };
        let page = case
            .with_conn(|connection| {
                query_timeline_filtered_for_case(connection, &case.case_root, &case.meta.id, query)
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
            })
            .unwrap();
        let total = page.total;
        ids.extend(page.items.into_iter().map(|item| item.id));
        let Some(next) = page.next_cursor else {
            return (ids, total);
        };
        cursor = Some(next);
        pages += 1;
        assert!(
            pages < 4_096,
            "timeline cursor pagination did not terminate"
        );
    }
}

fn assert_complete_in_order(ids: &[String], expected: &[String]) {
    assert_eq!(ids, expected);
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        ids.len(),
        "timeline pages must not repeat rows"
    );
}

#[test]
fn timeline_service_reads_fjall_projection_and_freezes_revision_cursor() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Fjall timeline", None).unwrap();
    let source = DataSource {
        id: DataSourceId("fjall-source".to_owned()),
        name: "Fjall source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: temporary.path().join("fjall-source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    case.with_conn(|connection| {
        let mut storage = DataSourceStorage::source_db(&source.id.0, Some("linux"), None);
        storage.import_state = "ready".to_owned();
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)
    })
    .unwrap();
    let source_conn = open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();

    let fjall = open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    let base = Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap();
    for (id, offset) in [("new", 2), ("old", 1), ("oldest", 0)] {
        fjall
            .timeline_upsert(
                &event(id, base + chrono::Duration::hours(offset)),
                PersistMode::Buffer,
            )
            .unwrap();
    }
    fjall.persist(PersistMode::SyncAll).unwrap();
    drop(fjall);
    let store = persistence_source::detail::open_store_for_write(&source_conn)
        .unwrap()
        .unwrap();
    persistence_source::fjall::seal_source_generation(
        &store,
        &source.id.0,
        "fixture",
        &"a".repeat(64),
    )
    .unwrap();
    drop(store);
    drop(source_conn);

    let incomplete = case
        .with_conn(|connection| {
            query_timeline_filtered_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 2,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: None,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect_err("an incomplete source projection must fail closed");
    assert!(incomplete
        .to_string()
        .contains("source details are not ready"));

    let fjall = open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    projection_support::mark_source_projection_complete(&fjall, &source.id.0);
    drop(fjall);
    let source_conn = open_source_db(&case.case_root, &source.id).unwrap();
    let revision_before =
        persistence_source::repositories::timeline_repo::TimelineRepo::new(&source_conn)
            .cursor_revision()
            .unwrap();

    let first = case
        .with_conn(|connection| {
            query_timeline_filtered_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 2,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: None,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["ds:fjall-source:new", "ds:fjall-source:old"]
    );
    let cursor = first.next_cursor.expect("remaining Fjall timeline row");

    let fjall = open_registered_fjall_source_read_write(&case.case_root, &source.id).unwrap();
    fjall
        .timeline_upsert(
            &event("inserted", base + chrono::Duration::hours(3)),
            PersistMode::SyncAll,
        )
        .unwrap();
    drop(fjall);
    let revision_after =
        persistence_source::repositories::timeline_repo::TimelineRepo::new(&source_conn)
            .cursor_revision()
            .unwrap();
    assert_ne!(
        revision_after, revision_before,
        "timeline mutation must advance the cursor revision"
    );

    let stale = case
        .with_conn(|connection| {
            query_timeline_filtered_for_case(
                connection,
                &case.case_root,
                &case.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 2,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap_err();
    assert!(stale.to_string().contains("timeline snapshot changed"));
}

#[test]
fn timeline_cursor_pages_concatenate_into_the_full_ordered_projection() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Fjall multi page", None).unwrap();
    let base = Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap();
    let first = (0..40)
        .map(|index| {
            source_event(
                "page-source-a",
                &format!("a-{index:03}"),
                base + chrono::Duration::minutes(index * 3),
            )
        })
        .collect::<Vec<_>>();
    let second = (0..40)
        .map(|index| {
            source_event(
                "page-source-b",
                &format!("b-{index:03}"),
                base + chrono::Duration::minutes(index * 3 + 1),
            )
        })
        .collect::<Vec<_>>();
    publish_source(&case, "page-source-a", &first);
    publish_source(&case, "page-source-b", &second);

    for event_type in [None, Some("FILE_CREATED")] {
        let (expected, total) = page_through(&case, event_type, 512);
        assert_eq!(total, 80);
        assert_eq!(expected.len(), 80);
        for page_size in [1u32, 31, 50, 63, 256] {
            let (ids, paged_total) = page_through(&case, event_type, page_size);
            assert_eq!(paged_total, 80, "page size {page_size} reported total");
            assert_eq!(ids.len(), 80, "page size {page_size} missed rows");
            assert_complete_in_order(&ids, &expected);
        }
    }
}

#[test]
fn timeline_cursor_keeps_equal_timestamp_order_across_pages() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Fjall tie pages", None).unwrap();
    let timestamp = Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap();
    for source_id in ["tie-source-a", "tie-source-b", "tie-source-c"] {
        let records = (0..12)
            .map(|index| source_event(source_id, &format!("event-{index:02}"), timestamp))
            .collect::<Vec<_>>();
        publish_source(&case, source_id, &records);
    }
    let expected = ["tie-source-a", "tie-source-b", "tie-source-c"]
        .into_iter()
        .flat_map(|source_id| (0..12).map(move |index| format!("ds:{source_id}:event-{index:02}")))
        .collect::<Vec<_>>();
    for (event_type, page_size) in [
        (None, 5u32),
        (None, 7),
        (None, 36),
        (Some("FILE_CREATED"), 7),
        (Some("FILE_CREATED"), 36),
    ] {
        let (ids, total) = page_through(&case, event_type, page_size);
        assert_eq!(total, 36, "page size {page_size} reported total");
        assert_complete_in_order(&ids, &expected);
    }
}
