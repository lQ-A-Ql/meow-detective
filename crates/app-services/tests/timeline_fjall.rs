use app_services::{
    case_service,
    source_db::{open_registered_fjall_source_read_write, open_source_db},
    timeline_service::{query_timeline_filtered_for_case, TimelineQuery},
};
use chrono::{TimeZone, Utc};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_fjall::{PersistMode, TimelineRecord};
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
    persistence_fjall::seal_source_generation(&store, &source.id.0, "fixture", &"a".repeat(64))
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
