//! Read-side timeline projection gate, status, and single-flight rebuild tests.
//!
//! Fixture sources register through the case DB and use the sealed-generation
//! layout; no real case or private sample is touched. A missing timeline marker
//! must behave exactly like the legacy complete case.

use app_services::{
    case_service,
    source_db::{open_fjall_source_for_build, open_source_db},
    timeline_service::{
        acquire_rebuild_guard, count_timeline_events_for_case, get_timeline_event_by_id_for_case,
        get_timeline_facets_for_case, get_timeline_projection_status, query_timeline_aggregated,
        query_timeline_filtered_for_case, rebuild_timeline_projection, TimelineProjectionPolicy,
        TimelineQuery, TimelineServiceError,
    },
};
use chrono::{TimeZone, Utc};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{
    timeline_projection_marker_envelope, KeyspaceName, PersistMode, TimelineProjectionState,
    TimelineRecord,
};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

#[path = "support/fjall_projection.rs"]
mod projection_support;

fn record(source_id: &str, id: &str, when: chrono::DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source_id.to_owned(),
        source_object_id: format!("file-{id}"),
        event_type: "FILE_CREATED".to_owned(),
        timestamp: when,
        title: id.to_owned(),
        description: format!("created {id}"),
        parser_id: Some("test.gate".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::new(),
    }
}

/// Register, open, populate, and seal a fixture source. When `marker` is given
/// the explicit timeline projection state marker is written; otherwise the
/// marker stays absent (legacy complete).
fn publish_source(
    case: &app_services::active_case::ActiveCase,
    source_id: &str,
    records: &[TimelineRecord],
    marker: Option<(TimelineProjectionState, Option<String>)>,
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
    let fjall = open_fjall_source_for_build(&source_conn, &source.id).unwrap();
    for record in records {
        fjall.timeline_upsert(record, PersistMode::Buffer).unwrap();
    }
    if let Some((state, reason)) = marker {
        let payload = persistence_source::fjall::TimelineProjectionStatePayload::new(
            state,
            format!("identity-{source_id}"),
            records.len() as u64,
            128 * 1024,
            256 * 1024,
            reason,
        );
        let (key, envelope) = timeline_projection_marker_envelope(source_id, &payload).unwrap();
        fjall
            .write_envelope(KeyspaceName::Meta, key, &envelope, PersistMode::Buffer)
            .unwrap();
    }
    fjall.persist(PersistMode::SyncAll).unwrap();
    let receipt = projection_support::mark_source_projection_complete(&fjall, source_id);
    persistence_source::fjall::seal_source_generation_with_control(
        fjall.source_store(),
        source_id,
        "fixture",
        &hex::encode(receipt.batch_digest),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(fjall);
    drop(source_conn);
}

fn run_with_conn<T>(
    case: &app_services::active_case::ActiveCase,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, TimelineServiceError>,
) -> T {
    case.with_conn(|connection| {
        f(connection).map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
    })
    .unwrap()
}

fn event_record(source_id: &str, id: &str, when: chrono::DateTime<Utc>) -> TimelineRecord {
    record(source_id, id, when)
}

#[test]
fn missing_marker_legacy_behavior_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "legacy marker", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    let records = vec![
        event_record("legacy", "e1", when),
        event_record("legacy", "e2", when),
    ];
    publish_source(&case, "legacy", &records, None);

    let page = run_with_conn(&case, |conn| {
        query_timeline_filtered_for_case(
            conn,
            &case.case_root,
            &case.meta.id,
            TimelineQuery {
                offset: 0,
                limit: 100,
                time_start: None,
                time_end: None,
                event_type: None,
                cursor: None,
            },
        )
    });
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 2);

    let total = run_with_conn(&case, |conn| {
        count_timeline_events_for_case(conn, &case.case_root, &case.meta.id)
    });
    assert_eq!(total, 2);

    let event = run_with_conn(&case, |conn| {
        get_timeline_event_by_id_for_case(conn, &case.case_root, &case.meta.id, "ds:legacy:e1")
    });
    assert!(event.is_some());

    let facets = run_with_conn(&case, |conn| {
        get_timeline_facets_for_case(conn, &case.case_root, &case.meta.id, None, None, None, 60)
    });
    assert_eq!(facets.total_events, 2);

    let status = run_with_conn(&case, |conn| {
        get_timeline_projection_status(conn, &case.case_root, &case.meta.id)
    });
    assert_eq!(
        status.state,
        transport::dto::TimelineProjectionStateDto::Complete
    );
    assert_eq!(status.sources.len(), 1);
    assert_eq!(status.sources[0].data_source_id, "legacy");
}

#[test]
fn building_source_is_skipped_by_cross_source_readers_but_blocks_explicit_queries() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "building gate", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(&case, "ready", &[event_record("ready", "r1", when)], None);
    publish_source(
        &case,
        "building",
        &[event_record("building", "b1", when)],
        Some((
            TimelineProjectionState::Building,
            Some("index in progress".to_owned()),
        )),
    );

    let page = run_with_conn(&case, |conn| {
        query_timeline_filtered_for_case(
            conn,
            &case.case_root,
            &case.meta.id,
            TimelineQuery {
                offset: 0,
                limit: 100,
                time_start: None,
                time_end: None,
                event_type: None,
                cursor: None,
            },
        )
    });
    assert_eq!(
        page.total, 1,
        "building source must not participate in total"
    );
    assert!(page.items.iter().all(|item| item.id.contains("ready")));

    let total = run_with_conn(&case, |conn| {
        count_timeline_events_for_case(conn, &case.case_root, &case.meta.id)
    });
    assert_eq!(total, 1, "building source must be excluded from count");

    let facets = run_with_conn(&case, |conn| {
        get_timeline_facets_for_case(conn, &case.case_root, &case.meta.id, None, None, None, 60)
    });
    assert_eq!(
        facets.total_events, 1,
        "building source must not mix into facets"
    );

    let connection = case.connection().expect("case connection");
    let by_id = get_timeline_event_by_id_for_case(
        &connection,
        &case.case_root,
        &case.meta.id,
        "ds:building:b1",
    );
    let Err(TimelineServiceError::ProjectionNotReady { status, .. }) = by_id else {
        panic!("explicit get-by-id on a building source must return ProjectionNotReady");
    };
    assert_eq!(status, "building");
}

#[test]
fn deferred_source_is_skipped_by_cross_source_readers() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "deferred gate", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(&case, "ready", &[event_record("ready", "r1", when)], None);
    publish_source(
        &case,
        "deferred",
        &[event_record("deferred", "d1", when)],
        Some((
            TimelineProjectionState::Deferred,
            Some("insufficient workspace".to_owned()),
        )),
    );

    let page = run_with_conn(&case, |conn| {
        query_timeline_filtered_for_case(
            conn,
            &case.case_root,
            &case.meta.id,
            TimelineQuery {
                offset: 0,
                limit: 100,
                time_start: None,
                time_end: None,
                event_type: None,
                cursor: None,
            },
        )
    });
    assert_eq!(page.total, 1);

    let total = run_with_conn(&case, |conn| {
        count_timeline_events_for_case(conn, &case.case_root, &case.meta.id)
    });
    assert_eq!(total, 1);

    let facets = run_with_conn(&case, |conn| {
        get_timeline_facets_for_case(conn, &case.case_root, &case.meta.id, None, None, None, 60)
    });
    assert_eq!(facets.total_events, 1);
}

#[test]
fn explicit_queries_return_typed_not_ready_error() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "typed error", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(
        &case,
        "building",
        &[event_record("building", "b1", when)],
        Some((
            TimelineProjectionState::Building,
            Some("still indexing".to_owned()),
        )),
    );

    let connection = case.connection().expect("case connection");
    let by_id = get_timeline_event_by_id_for_case(
        &connection,
        &case.case_root,
        &case.meta.id,
        "ds:building:b1",
    );
    let Err(TimelineServiceError::ProjectionNotReady {
        source_id, status, ..
    }) = by_id
    else {
        panic!("explicit get-by-id on a building source must return ProjectionNotReady");
    };
    assert_eq!(source_id, "building");
    assert_eq!(status, "building");

    let source_conn =
        open_source_db(&case.case_root, &DataSourceId("building".to_owned())).unwrap();
    let aggregated = query_timeline_aggregated(&source_conn, 0, 100);
    let Err(TimelineServiceError::ProjectionNotReady {
        source_id,
        status,
        reason,
    }) = aggregated
    else {
        panic!("expected ProjectionNotReady from aggregation");
    };
    assert_eq!(source_id, "building");
    assert_eq!(status, "building");
    assert_eq!(reason.as_deref(), Some("still indexing"));
}

#[test]
fn status_lists_skipped_sources_and_takes_worst_state() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "status worst", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(&case, "ready", &[event_record("ready", "r1", when)], None);
    publish_source(
        &case,
        "building",
        &[event_record("building", "b1", when)],
        Some((TimelineProjectionState::Building, None)),
    );
    publish_source(
        &case,
        "deferred",
        &[event_record("deferred", "d1", when)],
        Some((
            TimelineProjectionState::Deferred,
            Some("no budget".to_owned()),
        )),
    );

    let status = run_with_conn(&case, |conn| {
        get_timeline_projection_status(conn, &case.case_root, &case.meta.id)
    });
    assert_eq!(
        status.state,
        transport::dto::TimelineProjectionStateDto::Deferred,
        "deferred must win over building and complete"
    );
    let by_id: BTreeMap<String, _> = status
        .sources
        .iter()
        .map(|row| (row.data_source_id.clone(), row.state))
        .collect();
    assert_eq!(
        by_id.get("ready"),
        Some(&transport::dto::TimelineProjectionStateDto::Complete)
    );
    assert_eq!(
        by_id.get("building"),
        Some(&transport::dto::TimelineProjectionStateDto::Building)
    );
    assert_eq!(
        by_id.get("deferred"),
        Some(&transport::dto::TimelineProjectionStateDto::Deferred)
    );
    let deferred = status
        .sources
        .iter()
        .find(|row| row.data_source_id == "deferred")
        .unwrap();
    assert_eq!(deferred.reason.as_deref(), Some("no budget"));
    assert_eq!(deferred.inserted_events, 1);
}

#[test]
fn rebuild_second_concurrent_call_is_rejected_in_flight() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "single flight", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(
        &case,
        "building",
        &[event_record("building", "b1", when)],
        Some((TimelineProjectionState::Building, None)),
    );

    let _guard = acquire_rebuild_guard(&case.meta.id, Some(&DataSourceId("building".to_owned())));
    let cancel = AtomicBool::new(false);
    let connection = case.connection().expect("case connection");
    let result = rebuild_timeline_projection(
        &connection,
        &case.case_root,
        &case.meta.id,
        Some("building"),
        TimelineProjectionPolicy::default(),
        &cancel,
    );
    let Err(TimelineServiceError::RebuildInProgress { source_id }) = result else {
        panic!("expected RebuildInProgress while the guard is held");
    };
    assert_eq!(source_id.as_deref(), Some("building"));
    drop(_guard);
}

#[test]
fn case_wide_and_single_source_rebuild_guards_conflict_in_both_directions() {
    let case = domain::CaseId("guard-overlap".into());
    let source = DataSourceId("guard-source".into());
    let whole_case = acquire_rebuild_guard(&case, None).unwrap();
    assert!(acquire_rebuild_guard(&case, Some(&source)).is_err());
    drop(whole_case);
    let single_source = acquire_rebuild_guard(&case, Some(&source)).unwrap();
    assert!(acquire_rebuild_guard(&case, None).is_err());
    drop(single_source);
}

#[test]
fn rebuild_moves_fixture_source_back_to_complete() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "rebuild complete", None).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap();
    publish_source(
        &case,
        "building",
        &[event_record("building", "b1", when)],
        Some((TimelineProjectionState::Building, None)),
    );

    let cancel = AtomicBool::new(false);
    let connection = case.connection().expect("case connection");
    let status = rebuild_timeline_projection(
        &connection,
        &case.case_root,
        &case.meta.id,
        Some("building"),
        TimelineProjectionPolicy::default(),
        &cancel,
    )
    .expect("timeline rebuild must complete");
    let row = status
        .sources
        .iter()
        .find(|row| row.data_source_id == "building")
        .expect("rebuild status must list the target source");
    assert_eq!(
        row.state,
        transport::dto::TimelineProjectionStateDto::Complete
    );
}
