use app_services::timeline_service::{
    ensure_macb_timeline_projected, get_timeline_event_by_id_for_case, project_and_store_macb,
    query_timeline, query_timeline_aggregated, query_timeline_filtered_instrumented,
    query_timeline_for_case, query_timeline_instrumented, TimelineServiceError,
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
fn timeline_macb_projection_inserts_expected_events_and_handles_empty_input() {
    let conn = in_memory_db_with_timeline();
    let files = vec![
        make_file("a.txt", "/a.txt", true, true),
        make_file("b.txt", "/b.txt", true, false),
    ];
    assert_eq!(project_and_store_macb(&conn, &files).unwrap(), 5);
    assert_eq!(TimelineRepo::new(&conn).count().unwrap(), 5);
    assert_eq!(project_and_store_macb(&conn, &[]).unwrap(), 0);
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
fn timeline_instrumented_queries_report_rows_without_paths() {
    let conn = in_memory_db_with_timeline();
    let files = vec![make_file("test.txt", "/test.txt", true, true)];
    project_and_store_macb(&conn, &files).unwrap();

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
fn timeline_lazy_macb_projection_is_idempotent() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(TIMELINE_SCHEMA).unwrap();
    conn.execute_batch(
        "ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
         ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
         ALTER TABLE timeline_events ADD COLUMN confidence REAL;
         ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;
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
         INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES ('ds-1', 'case-1', 'sample', 'Raw', '/sample.raw');
         INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, created_at, modified_at, accessed_at)
         VALUES ('file-1', 'ds-1', '/file.txt', 'file.txt', 'file',
                 '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z', '2026-01-03T00:00:00Z');",
    )
    .unwrap();

    let first = ensure_macb_timeline_projected(&conn).unwrap();
    assert_eq!(first.inserted_count, 3);
    assert!(!first.already_projected);
    let second = ensure_macb_timeline_projected(&conn).unwrap();
    assert_eq!(second.inserted_count, 0);
    assert!(second.already_projected);

    let page = query_timeline(&conn, 0, 100).unwrap();
    assert_eq!(page.total, 3);
    assert!(page
        .items
        .iter()
        .any(|event| event.id == "macb:file-1:FILE_CREATED"));
}
