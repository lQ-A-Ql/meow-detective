use app_services::{case_service, source_db, timeline_service};
use chrono::{TimeZone, Utc};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::path::Path;
use timeline_service::{TimelineQuery, TimelineServiceError};

fn register_source(conn: &Connection, root: &Path, case: &CaseId, id: &str) -> Connection {
    let source = DataSource {
        id: DataSourceId(id.into()),
        name: id.into(),
        kind: DataSourceKind::Raw,
        source_path: root.join(format!("{id}.raw")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(id, Some("windows"), None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(conn)
        .insert_with_storage(case, &source, &storage)
        .unwrap();
    let database = source_db::open_source_db(root, &source.id).unwrap();
    DataSourceRepo::new(&database)
        .upsert_source_local_metadata(case, &source)
        .unwrap();
    database
}

fn event(id: String) -> domain::TimelineEvent {
    domain::TimelineEvent {
        id: domain::TimelineEventId(id.clone()),
        source_object_id: "file".into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: Utc.with_ymd_and_hms(2026, 10, 18, 0, 0, 0).unwrap(),
        title: id,
        description: String::new(),
        parser_id: Some("test.cursor".into()),
        parser_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        attrs: Default::default(),
    }
}

#[test]
fn equal_timestamp_pages_cross_lookahead_boundaries_and_exclude_later_backdated_events() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "cursor-ties", None).unwrap();
    active
        .with_conn(|conn| {
            for id in ["c", "a", "b"] {
                let db = register_source(conn, &active.case_root, &active.meta.id, id);
                let events = (0..80)
                    .rev()
                    .map(|n| event(format!("event-{n:03}")))
                    .collect::<Vec<_>>();
                TimelineRepo::new(&db).insert_batch_with_case(&events, &active.meta.id.0)?;
            }
            let mut cursor = None;
            let mut ids = Vec::new();
            for page in 0..240 {
                let mut query = TimelineQuery::unfiltered(0, [1, 31, 63, 7][page % 4]);
                query.cursor = cursor.as_deref();
                let result = timeline_service::query_timeline_filtered_for_case(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                    query,
                )
                .unwrap();
                assert_eq!(result.total, 240);
                ids.extend(result.items.into_iter().map(|row| row.id));
                cursor = result.next_cursor;
                if page == 0 {
                    let db =
                        source_db::open_source_db(&active.case_root, &DataSourceId("a".into()))?;
                    let mut appended = event("appended-backdated".into());
                    appended.timestamp -= chrono::Duration::days(10);
                    TimelineRepo::new(&db)
                        .insert_batch_with_case(&[appended], &active.meta.id.0)?;
                }
                if cursor.is_none() {
                    break;
                }
            }
            let expected = ["a", "b", "c"]
                .into_iter()
                .flat_map(|id| (0..80).map(move |n| format!("ds:{id}:event-{n:03}")))
                .collect::<Vec<_>>();
            assert_eq!(ids, expected);
            assert!(cursor.is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn source_retirement_invalidates_existing_cursor_with_a_validation_error() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "cursor-retired-source", None).unwrap();
    active.with_conn(|conn| {
        for id in ["a", "b"] {
            let db = register_source(conn, &active.case_root, &active.meta.id, id);
            TimelineRepo::new(&db).insert_batch_with_case(
                &[event("first".into()), event("last".into())], &active.meta.id.0,
            )?;
        }
        let first = timeline_service::query_timeline_for_case(
            conn, &active.case_root, &active.meta.id, 0, 1,
        ).unwrap();
        let cursor = first.next_cursor.unwrap();
        DataSourceRepo::new(conn).update_import_state(&DataSourceId("b".into()), "failed", None)?;
        let mut query = TimelineQuery::unfiltered(0, 20);
        query.cursor = Some(&cursor);
        let error = timeline_service::query_timeline_filtered_for_case(
            conn, &active.case_root, &active.meta.id, query,
        ).unwrap_err();
        assert!(matches!(error, TimelineServiceError::InvalidInput(ref message) if message.contains("data source set changed")));
        let fresh = timeline_service::query_timeline_for_case(
            conn, &active.case_root, &active.meta.id, 0, 20,
        ).unwrap();
        assert_eq!(fresh.total, 2);
        assert!(fresh.items.iter().all(|row| row.id.starts_with("ds:a:")));
        Ok(())
    }).unwrap();
}
