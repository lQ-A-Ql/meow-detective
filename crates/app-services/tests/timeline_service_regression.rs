use app_services::timeline_service::{
    get_timeline_event_by_id_for_case, get_timeline_facets_for_case,
    materialize_file_activity_unknown, project_and_store_file_activity, query_timeline,
    query_timeline_aggregated, query_timeline_filtered_for_case,
    query_timeline_filtered_instrumented, query_timeline_for_case, query_timeline_instrumented,
    TimelineQuery, TimelineServiceError,
};
use chrono::{TimeZone, Utc};
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    timeline_repo::TimelineRepo,
};
use transport::dto::PerformanceReportDto;

const TIMELINE_SCHEMA: &str =
    include_str!("../../persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql");

const MACB_SOURCE_SCHEMA: &str = r#"
CREATE TABLE data_sources (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    size INTEGER,
    imported_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE file_entries (
    id TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT,
    data_source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT,
    hash_sha256 TEXT
);
"#;

fn in_memory_db_with_timeline() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(TIMELINE_SCHEMA).unwrap();
    conn.execute_batch(
        "ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
         ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
         ALTER TABLE timeline_events ADD COLUMN confidence REAL;
         ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;",
    )
    .unwrap();
    conn.execute_batch(MACB_SOURCE_SCHEMA).unwrap();
    conn
}

fn in_memory_case_db_with_source() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = domain::CaseId("case-1".to_string());
    let case = domain::CaseMeta {
        id: case_id.clone(),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
        .create(&case)
        .unwrap();
    let source = domain::DataSource {
        id: DataSourceId("ds-1".to_string()),
        name: "source".to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: std::path::PathBuf::from("D:/source"),
        imported_at: Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
        .insert(&case_id, &source)
        .unwrap();
    conn.execute_batch("UPDATE data_sources SET import_state='ready', platform='linux'")
        .unwrap();
    conn
}

fn make_file(name: &str, path: &str, created: bool, modified: bool) -> FileEntry {
    FileEntry {
        id: FileEntryId(uuid::Uuid::new_v4().to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds-1".to_string()),
        path: path.to_string(),
        name: name.to_string(),
        entry_type: EntryType::File,
        size: Some(1024),
        ext: Some("txt".to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: created.then(|| Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap()),
        modified_at: modified.then(|| Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap()),
        accessed_at: Some(Utc.with_ymd_and_hms(2024, 6, 15, 14, 0, 0).unwrap()),
        changed_at: None,
        hash_sha256: None,
    }
}

fn insert_events(conn: &rusqlite::Connection, rows: &[(&str, &str, &str, &str)]) {
    let events: Vec<domain::TimelineEvent> = rows
        .iter()
        .map(|(id, event_type, description, ts)| domain::TimelineEvent {
            id: domain::TimelineEventId(id.to_string()),
            source_object_id: "src-1".to_string(),
            event_type: event_type.to_string(),
            timestamp: chrono::DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&Utc),
            title: format!("{event_type} event"),
            description: description.to_string(),
            parser_id: None,
            parser_version: None,
            confidence: None,
            source_attribution: None,
            attrs: Default::default(),
        })
        .collect();
    TimelineRepo::new(conn).insert_batch(&events).unwrap();
}

fn metric_value(report: &PerformanceReportDto, key: &str) -> Option<f64> {
    report
        .metrics
        .iter()
        .find(|metric| metric.key == key)
        .map(|metric| metric.value)
}

fn register_ready_source(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    source_id: &str,
) -> persistence_sqlite::DbResult<rusqlite::Connection> {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{source_id}.E01")),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_conn).insert_with_storage(case_id, &source, &storage)?;
    let source_conn = app_services::source_db::open_source_db(case_root, &source.id)?;
    DataSourceRepo::new(&source_conn).upsert_source_local_metadata(case_id, &source)?;
    Ok(source_conn)
}

fn timeline_event(id: &str, timestamp: chrono::DateTime<Utc>) -> domain::TimelineEvent {
    domain::TimelineEvent {
        id: domain::TimelineEventId(id.to_string()),
        source_object_id: format!("file-{id}"),
        event_type: "FILE_CREATED".to_string(),
        timestamp,
        title: id.to_string(),
        description: id.to_string(),
        parser_id: Some("test.timeline".to_string()),
        parser_version: None,
        confidence: Some(1.0),
        source_attribution: None,
        attrs: Default::default(),
    }
}

#[test]
fn timeline_file_activity_projection_inserts_expected_events_and_handles_empty_input() {
    let conn = in_memory_db_with_timeline();
    let files = vec![
        make_file("a.txt", "/a.txt", true, true),
        make_file("b.txt", "/b.txt", true, false),
    ];
    assert_eq!(project_and_store_file_activity(&conn, &files).unwrap(), 5);
    assert_eq!(TimelineRepo::new(&conn).count().unwrap(), 5);
    let event_types = conn
        .prepare("SELECT event_type FROM timeline_events ORDER BY event_type")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        event_types,
        vec![
            "FILE_ACCESSED",
            "FILE_ACCESSED",
            "FILE_CREATED",
            "FILE_CREATED",
            "FILE_MODIFIED",
        ]
    );
    assert_eq!(project_and_store_file_activity(&conn, &[]).unwrap(), 0);
}

#[test]
fn timeline_source_database_query_wraps_event_and_source_object_ids() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db_with_source();
    let source_id = DataSourceId("ds-1".to_string());
    let source_conn = app_services::source_db::open_source_db(tmp.path(), &source_id).unwrap();
    let event = domain::TimelineEvent {
        id: domain::TimelineEventId("event-1".to_string()),
        source_object_id: "file-1".to_string(),
        event_type: "FILE_CREATED".to_string(),
        timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
        title: "created".to_string(),
        description: "created file".to_string(),
        parser_id: Some("test.parser".to_string()),
        parser_version: None,
        confidence: Some(1.0),
        source_attribution: None,
        attrs: Default::default(),
    };
    TimelineRepo::new(&source_conn)
        .insert_batch_with_case(&[event], "case-1")
        .unwrap();

    let case_id = domain::CaseId("case-1".to_string());
    let page = query_timeline_for_case(&case_conn, tmp.path(), &case_id, 0, 10).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "ds:ds-1:event-1");
    assert_eq!(page.items[0].source_object_id, "ds:ds-1:file-1");

    let event =
        get_timeline_event_by_id_for_case(&case_conn, tmp.path(), &case_id, "ds:ds-1:event-1")
            .unwrap()
            .unwrap();
    assert_eq!(event.event_type, "FILE_CREATED");
}

#[test]
fn timeline_facets_aggregate_ready_source_databases_with_epoch_filters() {
    let temp = tempfile::TempDir::new().unwrap();
    let active =
        app_services::case_service::create_case(temp.path(), "timeline-facets", Some("tester"))
            .unwrap();
    active
        .with_conn(|case_conn| {
            let source_a =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let source_b =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-b")?;
            let first = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            let second = first + chrono::Duration::hours(2);
            TimelineRepo::new(&source_a).insert_batch_with_case(
                &[timeline_event("a-1", first), timeline_event("a-2", second)],
                &active.meta.id.0,
            )?;
            let mut other = timeline_event("b-1", first + chrono::Duration::hours(1));
            other.event_type = "REGISTRY_HIVE_LAST_WRITE".to_string();
            TimelineRepo::new(&source_b).insert_batch_with_case(&[other], &active.meta.id.0)?;
            drop(source_a);
            drop(source_b);

            let facets = get_timeline_facets_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                None,
                None,
                None,
                20,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(facets.total_events, 3);
            assert_eq!(
                facets.start_ts.as_deref(),
                Some("2026-02-03T04:05:06+00:00")
            );
            assert_eq!(facets.end_ts.as_deref(), Some("2026-02-03T06:05:06+00:00"));
            assert_eq!(facets.data_sources.len(), 2);
            assert_eq!(facets.event_types.len(), 2);
            assert_eq!(
                facets
                    .histogram
                    .iter()
                    .map(|bucket| bucket.count)
                    .sum::<u64>(),
                3
            );
            for adjacent in facets.histogram.windows(2) {
                let previous_end = chrono::DateTime::parse_from_rfc3339(&adjacent[0].end_ts)
                    .unwrap()
                    .timestamp();
                let next_start = chrono::DateTime::parse_from_rfc3339(&adjacent[1].start_ts)
                    .unwrap()
                    .timestamp();
                assert_eq!(previous_end + 1, next_start);
            }

            let filtered = get_timeline_facets_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                Some("2026-02-03T05:00:00Z"),
                None,
                Some("FILE_CREATED"),
                20,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(filtered.total_events, 1);
            assert_eq!(filtered.data_sources[0].value, "source-a");
            assert_eq!(
                filtered
                    .event_types
                    .iter()
                    .map(|facet| facet.value.as_str())
                    .collect::<Vec<_>>(),
                vec!["FILE_CREATED", "REGISTRY_HIVE_LAST_WRITE"]
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_facets_limit_buckets_to_short_epoch_ranges() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        temp.path(),
        "timeline-short-facets",
        Some("tester"),
    )
    .unwrap();
    active
        .with_conn(|case_conn| {
            let source =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let first = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            TimelineRepo::new(&source).insert_batch_with_case(
                &[
                    timeline_event("a-1", first),
                    timeline_event("a-2", first + chrono::Duration::seconds(1)),
                ],
                &active.meta.id.0,
            )?;
            drop(source);

            let facets = get_timeline_facets_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                None,
                None,
                None,
                20,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert_eq!(facets.histogram.len(), 2);
            assert_eq!(
                facets
                    .histogram
                    .iter()
                    .map(|bucket| bucket.count)
                    .sum::<u64>(),
                2
            );
            assert_eq!(facets.histogram[0].start_ts, facets.histogram[0].end_ts);
            assert_eq!(facets.histogram[1].start_ts, facets.histogram[1].end_ts);
            assert_ne!(facets.histogram[0].start_ts, facets.histogram[1].start_ts);
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_source_database_query_rejects_unscoped_event_ids() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = in_memory_case_db_with_source();
    let error = get_timeline_event_by_id_for_case(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "event-1",
    )
    .unwrap_err();
    assert!(matches!(error, TimelineServiceError::InvalidInput(_)));
    assert!(error.to_string().contains("ds:<dataSourceId>:<localId>"));
}

#[test]
fn timeline_case_pagination_is_stable_across_source_identity_ties() {
    let temp = tempfile::TempDir::new().unwrap();
    let active =
        app_services::case_service::create_case(temp.path(), "timeline-order", Some("tester"))
            .unwrap();
    active
        .with_conn(|case_conn| {
            let source_b =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-b")?;
            let source_a =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let timestamp = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            TimelineRepo::new(&source_b).insert_batch_with_case(
                &[timeline_event("event-0", timestamp)],
                &active.meta.id.0,
            )?;
            TimelineRepo::new(&source_a).insert_batch_with_case(
                &[
                    timeline_event("event-2", timestamp),
                    timeline_event("event-1", timestamp),
                ],
                &active.meta.id.0,
            )?;

            let page = query_timeline_for_case(case_conn, &active.case_root, &active.meta.id, 1, 1)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(page.total, 3);
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, "ds:source-a:event-2");
            assert_eq!(page.items[0].source_object_id, "ds:source-a:file-event-2");
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_cursor_freezes_snapshot_and_tracks_only_consumed_keys() {
    let temp = tempfile::TempDir::new().unwrap();
    let active =
        app_services::case_service::create_case(temp.path(), "timeline-cursor", Some("tester"))
            .unwrap();
    active
        .with_conn(|case_conn| {
            let source_a =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let source_b =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-b")?;
            let timestamp = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            TimelineRepo::new(&source_a).insert_batch_with_case(
                &[
                    timeline_event("same-a", timestamp),
                    timeline_event("same-b", timestamp),
                    timeline_event("old", timestamp - chrono::Duration::days(1)),
                ],
                &active.meta.id.0,
            )?;
            TimelineRepo::new(&source_b).insert_batch_with_case(
                &[timeline_event("same-a", timestamp)],
                &active.meta.id.0,
            )?;
            drop(source_a);
            drop(source_b);

            let first = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 2,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: None,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(first.total, 4);
            assert_eq!(
                first
                    .items
                    .iter()
                    .map(|event| event.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["ds:source-a:same-a", "ds:source-a:same-b"]
            );
            let cursor = first
                .next_cursor
                .expect("cursor for remaining snapshot rows");

            let source_b = app_services::source_db::open_source_db(
                &active.case_root,
                &DataSourceId("source-b".to_string()),
            )?;
            TimelineRepo::new(&source_b).insert_batch_with_case(
                &[timeline_event(
                    "inserted-later",
                    timestamp + chrono::Duration::days(1),
                )],
                &active.meta.id.0,
            )?;
            drop(source_b);

            let second = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 10,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(second.total, 4);
            assert_eq!(
                second
                    .items
                    .iter()
                    .map(|event| event.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["ds:source-b:same-a", "ds:source-a:old"]
            );
            assert!(second.next_cursor.is_none());

            let wrong_filter = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 10,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_MODIFIED"),
                    cursor: Some(&cursor),
                },
            )
            .unwrap_err();
            assert!(matches!(
                wrong_filter,
                TimelineServiceError::InvalidInput(_)
            ));

            let mut tampered = cursor.clone().into_bytes();
            let payload_index = tampered.iter().position(|byte| *byte == b'.').unwrap() + 1;
            tampered[payload_index] = if tampered[payload_index] == b'A' {
                b'B'
            } else {
                b'A'
            };
            let tampered = String::from_utf8(tampered).unwrap();
            let error = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 10,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&tampered),
                },
            )
            .unwrap_err();
            assert!(matches!(error, TimelineServiceError::InvalidInput(_)));

            drop(register_ready_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "source-c",
            )?);
            let stale = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .unwrap_err();
            assert!(matches!(stale, TimelineServiceError::InvalidInput(_)));
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_cursor_rejects_offset_and_deleted_unconsumed_rows() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        temp.path(),
        "timeline-stale-cursor",
        Some("tester"),
    )
    .unwrap();
    active
        .with_conn(|case_conn| {
            let source =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let timestamp = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            TimelineRepo::new(&source).insert_batch_with_case(
                &[
                    timeline_event("newest", timestamp),
                    timeline_event("middle", timestamp - chrono::Duration::hours(1)),
                    timeline_event("oldest", timestamp - chrono::Duration::hours(2)),
                ],
                &active.meta.id.0,
            )?;
            drop(source);

            let first = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: None,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let cursor = first.next_cursor.expect("cursor for remaining rows");

            let offset_error = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 1,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .unwrap_err();
            assert!(matches!(
                offset_error,
                TimelineServiceError::InvalidInput(_)
            ));

            let source = app_services::source_db::open_source_db(
                &active.case_root,
                &DataSourceId("source-a".to_string()),
            )?;
            source
                .execute("DELETE FROM timeline_events WHERE id = 'oldest'", [])
                .map_err(persistence_sqlite::DbError::from)?;
            drop(source);

            let stale = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .unwrap_err();
            match stale {
                TimelineServiceError::InvalidInput(message) => {
                    assert!(message.contains("timeline snapshot changed"));
                }
                other => panic!("expected stale cursor validation error, got {other}"),
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_cursor_rejects_equal_count_replacement() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        temp.path(),
        "timeline-equal-count-replacement",
        Some("tester"),
    )
    .unwrap();
    active
        .with_conn(|case_conn| {
            let source =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let timestamp = Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
            let mut originals = vec![
                timeline_event("newest", timestamp),
                timeline_event("middle", timestamp - chrono::Duration::hours(1)),
                timeline_event("oldest", timestamp - chrono::Duration::hours(2)),
            ];
            for event in &mut originals {
                event.source_object_id = "shared-source".to_string();
            }
            TimelineRepo::new(&source).insert_batch_with_case(&originals, &active.meta.id.0)?;
            drop(source);

            let first = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: None,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let cursor = first.next_cursor.expect("cursor for remaining rows");

            let source = app_services::source_db::open_source_db(
                &active.case_root,
                &DataSourceId("source-a".to_string()),
            )?;
            let transaction = source.unchecked_transaction()?;
            let repository = TimelineRepo::new(&transaction);
            assert_eq!(
                repository.delete_analysis_outputs_in_transaction("shared-source", "test.")?,
                3
            );
            let mut replacements = vec![
                timeline_event("replacement-newest", timestamp),
                timeline_event("replacement-middle", timestamp - chrono::Duration::hours(1)),
                timeline_event("replacement-oldest", timestamp - chrono::Duration::hours(2)),
            ];
            for event in &mut replacements {
                event.source_object_id = "shared-source".to_string();
            }
            repository.insert_batch_with_case_in_transaction(&replacements, &active.meta.id.0)?;
            transaction.commit()?;

            let stale = query_timeline_filtered_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                TimelineQuery {
                    offset: 0,
                    limit: 1,
                    time_start: None,
                    time_end: None,
                    event_type: Some("FILE_CREATED"),
                    cursor: Some(&cursor),
                },
            )
            .unwrap_err();
            match stale {
                TimelineServiceError::InvalidInput(message) => {
                    assert!(message.contains("timeline snapshot changed"));
                }
                other => panic!("expected stale cursor validation error, got {other}"),
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_case_deep_page_refills_sources_in_bounded_batches() {
    let temp = tempfile::TempDir::new().unwrap();
    let active =
        app_services::case_service::create_case(temp.path(), "timeline-deep", Some("tester"))
            .unwrap();
    active
        .with_conn(|case_conn| {
            let first =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-a")?;
            let second =
                register_ready_source(case_conn, &active.case_root, &active.meta.id, "source-b")?;
            let first_base = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
            let second_base = Utc.with_ymd_and_hms(2026, 6, 2, 0, 0, 0).unwrap();
            let first_events = (0..300)
                .map(|index| {
                    timeline_event(
                        &format!("first-{index:03}"),
                        first_base + chrono::Duration::seconds(index),
                    )
                })
                .collect::<Vec<_>>();
            let second_events = (0..300)
                .map(|index| {
                    timeline_event(
                        &format!("second-{index:03}"),
                        second_base + chrono::Duration::seconds(index),
                    )
                })
                .collect::<Vec<_>>();
            TimelineRepo::new(&first).insert_batch_with_case(&first_events, &active.meta.id.0)?;
            TimelineRepo::new(&second).insert_batch_with_case(&second_events, &active.meta.id.0)?;

            let page =
                query_timeline_for_case(case_conn, &active.case_root, &active.meta.id, 520, 40)
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert_eq!(page.total, 600);
            assert_eq!(page.items.len(), 40);
            assert!(page
                .items
                .iter()
                .all(|event| event.id.starts_with("ds:source-a:first-")));
            assert_eq!(page.items.first().unwrap().id, "ds:source-a:first-079");
            assert_eq!(page.items.last().unwrap().id, "ds:source-a:first-040");
            Ok(())
        })
        .unwrap();
}

#[test]
fn timeline_instrumented_queries_report_rows_without_paths() {
    let conn = in_memory_db_with_timeline();
    insert_events(
        &conn,
        &[
            ("event-1", "FILE_CREATED", "created", "2026-01-01T00:00:00Z"),
            (
                "event-2",
                "FILE_MODIFIED",
                "modified",
                "2026-01-02T00:00:00Z",
            ),
            (
                "event-3",
                "FILE_ACCESSED",
                "accessed",
                "2026-01-03T00:00:00Z",
            ),
        ],
    );

    let result = query_timeline_instrumented(&conn, 0, 100).unwrap();
    assert_eq!(result.page.items.len(), 3);
    assert_eq!(
        metric_value(&result.performance_report, "timeline.query.rows"),
        Some(3.0)
    );
    assert_eq!(
        metric_value(&result.performance_report, "timeline.query.totalRows"),
        Some(3.0)
    );
    assert!(result
        .performance_report
        .metrics
        .iter()
        .all(|metric| !metric.key.contains("path")));

    let filtered =
        query_timeline_filtered_instrumented(&conn, 0, 100, None, None, Some("FILE_CREATED"))
            .unwrap();
    assert_eq!(filtered.page.items.len(), 1);
    assert_eq!(
        metric_value(&filtered.performance_report, "timeline.query.rows"),
        Some(1.0)
    );
}

#[test]
fn timeline_aggregation_preserves_group_counts_ranges_and_pagination() {
    let conn = in_memory_db_with_timeline();
    insert_events(
        &conn,
        &[
            (
                "e1",
                "FILE_CREATED",
                "File created: /a.txt",
                "2025-01-01T10:00:00Z",
            ),
            (
                "e2",
                "FILE_CREATED",
                "File created: /b.txt",
                "2025-01-01T11:00:00Z",
            ),
            (
                "e3",
                "FILE_MODIFIED",
                "File modified: /a.txt",
                "2025-01-01T12:00:00Z",
            ),
            (
                "e4",
                "FILE_MODIFIED",
                "File modified: /a.txt",
                "2025-01-01T12:30:00Z",
            ),
            (
                "e5",
                "FILE_ACCESSED",
                "File accessed: /c.txt",
                "2025-01-01T13:00:00Z",
            ),
        ],
    );

    let result = query_timeline_aggregated(&conn, 0, 50).unwrap();
    assert_eq!(result.stripes_by_type.len(), 3);
    let modified = &result.stripes_by_type["FILE_MODIFIED"];
    assert_eq!(modified.total_events, 2);
    assert_eq!(modified.clusters.len(), 1);
    assert_eq!(modified.clusters[0].count, 2);
    assert!(modified.clusters[0]
        .first_ts
        .starts_with("2025-01-01T12:00:00"));
    assert!(modified.clusters[0]
        .last_ts
        .starts_with("2025-01-01T12:30:00"));
    assert!(modified.clusters[0].sample_event_ids.len() <= 5);

    let page = query_timeline_aggregated(&conn, 0, 2).unwrap();
    let cluster_count: usize = page
        .stripes_by_type
        .values()
        .map(|stripe| stripe.clusters.len())
        .sum();
    assert_eq!(cluster_count, 2);
}

#[test]
fn timeline_aggregation_keeps_sample_ids_within_the_cluster() {
    let conn = in_memory_db_with_timeline();
    insert_events(
        &conn,
        &[
            (
                "e1",
                "FILE_MODIFIED",
                "File modified: /shared.txt",
                "2025-06-01T08:00:00Z",
            ),
            (
                "e2",
                "FILE_MODIFIED",
                "File modified: /shared.txt",
                "2025-06-02T12:00:00Z",
            ),
            (
                "e3",
                "FILE_MODIFIED",
                "File modified: /shared.txt",
                "2025-06-03T16:00:00Z",
            ),
        ],
    );

    let result = query_timeline_aggregated(&conn, 0, 10).unwrap();
    let cluster = &result.stripes_by_type["FILE_MODIFIED"].clusters[0];
    assert_eq!(cluster.count, 3);
    assert!(!cluster.sample_event_ids.is_empty());
    assert!(cluster.sample_event_ids.len() <= 5);
    for sample_id in &cluster.sample_event_ids {
        assert!(["e1", "e2", "e3"].contains(&sample_id.as_str()));
    }
}

#[test]
fn timeline_large_aggregation_remains_bounded() {
    let conn = in_memory_db_with_timeline();
    let event_types = [
        "FILE_CREATED",
        "FILE_MODIFIED",
        "FILE_ACCESSED",
        "FILE_METADATA_CHANGED",
    ];
    let events: Vec<domain::TimelineEvent> = (0..10_000u32)
        .map(|index| {
            let event_type = event_types[index as usize % event_types.len()];
            let hour = index % 24;
            domain::TimelineEvent {
                id: domain::TimelineEventId(format!("e{index:05}")),
                source_object_id: "src-1".to_string(),
                event_type: event_type.to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339(&format!(
                    "2025-06-01T{hour:02}:00:00Z"
                ))
                .unwrap()
                .with_timezone(&Utc),
                title: format!("{event_type} event"),
                description: format!("Test: /path/{}.txt", index % 100),
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: Default::default(),
            }
        })
        .collect();
    TimelineRepo::new(&conn).insert_batch(&events).unwrap();

    let started = std::time::Instant::now();
    let result = query_timeline_aggregated(&conn, 0, 20).unwrap();
    let total_clusters: usize = result
        .stripes_by_type
        .values()
        .map(|stripe| stripe.clusters.len())
        .sum();
    assert!(total_clusters <= 20);
    assert!(result
        .stripes_by_type
        .values()
        .all(|stripe| stripe.total_events >= stripe.clusters.len() as u64));
    assert!(
        started.elapsed().as_secs() < 5,
        "10K-row aggregation exceeded the five-second regression budget"
    );
}

#[test]
fn timeline_file_activity_materialization_is_idempotent() {
    let conn = in_memory_db_with_timeline();
    conn.execute_batch(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES ('ds-1', 'case-1', 'sample', 'Raw', '/sample.raw');
         INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, created_at, modified_at, accessed_at)
         VALUES ('file-1', 'ds-1', '/file.txt', 'file.txt', 'file',
                 '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z', '2026-01-03T00:00:00Z');",
    )
    .unwrap();

    let first = materialize_file_activity_unknown(&conn).unwrap();
    assert_eq!(first.inserted_count, 3);
    assert!(!first.already_projected);
    let second = materialize_file_activity_unknown(&conn).unwrap();
    assert_eq!(second.inserted_count, 0);
    assert!(second.already_projected);

    let page = query_timeline(&conn, 0, 100).unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        page.items
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["FILE_ACCESSED", "FILE_MODIFIED", "FILE_CREATED"]
    );
}
