use std::cell::Cell;
use std::time::Duration;

use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState,
    },
    runner,
};

use super::phase_execution::run_phase;
use super::phase_runner::ProcessingPhaseRunner;
use super::*;

const SOURCE_ID: &str = "derived-test-source";
const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn setup_case_db() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-1', 'Derived Finalizer')",
        [],
    )
    .expect("insert case");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES (?1, 'case-1', 'VM disk', 'ceph_rbd', 'ceph-rbd://cluster/image')",
        [SOURCE_ID],
    )
    .expect("insert derived source");
    conn
}

fn setup_file_case_db(path: &std::path::Path) -> rusqlite::Connection {
    let conn = persistence_sqlite::open_or_create(path).expect("open file case database");
    runner::run_all(&conn).expect("run migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-1', 'Derived Finalizer')",
        [],
    )
    .expect("insert case");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES (?1, 'case-1', 'VM disk', 'ceph_rbd', 'ceph-rbd://cluster/image')",
        [SOURCE_ID],
    )
    .expect("insert derived source");
    conn
}

#[test]
fn failed_phase_is_persisted_without_aborting_the_report() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let mut report = DerivedFinalizationReport::default();

    run_phase(&runner, ProcessingPhase::Artifacts, &mut report, || {
        Err("artifact parser failed".to_string())
    });

    let stored = DataSourceProcessingPhaseRepo::new(&conn)
        .find(&source_id, ProcessingPhase::Artifacts)
        .expect("query artifacts phase")
        .expect("artifacts phase exists");
    assert_eq!(stored.state, ProcessingPhaseState::Failed);
    assert_eq!(stored.last_error.as_deref(), Some("artifact parser failed"));
    assert_eq!(report.failed_count(), 1);
}

#[test]
fn failed_phase_can_retry_without_resetting_its_identity() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let mut first = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Search, &mut first, || {
        Err("index unavailable".to_string())
    });

    let mut retry = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Search, &mut retry, || {
        Ok(r#"{"indexedCount":12}"#.to_string())
    });

    let stored = DataSourceProcessingPhaseRepo::new(&conn)
        .find(&source_id, ProcessingPhase::Search)
        .expect("query search phase")
        .expect("search phase exists");
    assert_eq!(stored.state, ProcessingPhaseState::Ready);
    assert_eq!(stored.stats_json, r#"{"indexedCount":12}"#);
    assert_eq!(retry.failed_count(), 0);
}

#[test]
fn ready_phase_is_idempotent_and_does_not_rerun_work() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let mut first = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Graph, &mut first, || {
        Ok(r#"{"projected":true}"#.to_string())
    });

    let calls = Cell::new(0);
    let mut second = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Graph, &mut second, || {
        calls.set(calls.get() + 1);
        Ok("{}".to_string())
    });

    assert_eq!(calls.get(), 0);
    assert_eq!(second.phases.len(), 1);
    assert_eq!(second.phases[0].state, ProcessingPhaseState::Ready);
}

#[test]
fn projection_retry_only_reruns_the_failed_search_action() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let timeline_calls = Cell::new(0);
    let search_calls = Cell::new(0);
    let mut first = DerivedFinalizationReport::default();

    run_phase(&runner, ProcessingPhase::Timeline, &mut first, || {
        timeline_calls.set(timeline_calls.get() + 1);
        Ok(r#"{"macbInsertedCount":4}"#.to_string())
    });
    run_phase(&runner, ProcessingPhase::Search, &mut first, || {
        search_calls.set(search_calls.get() + 1);
        Err("search writer unavailable".to_string())
    });

    let mut retry = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Timeline, &mut retry, || {
        timeline_calls.set(timeline_calls.get() + 1);
        Ok(r#"{"macbInsertedCount":99}"#.to_string())
    });
    run_phase(&runner, ProcessingPhase::Search, &mut retry, || {
        search_calls.set(search_calls.get() + 1);
        Ok(r#"{"eligibleCount":3,"indexedCount":3,"skippedCount":0,"failedCount":0}"#.to_string())
    });

    assert_eq!(timeline_calls.get(), 1);
    assert_eq!(search_calls.get(), 2);
    let repository = DataSourceProcessingPhaseRepo::new(&conn);
    let timeline = repository
        .find(&source_id, ProcessingPhase::Timeline)
        .expect("query timeline phase")
        .expect("timeline phase exists");
    let search = repository
        .find(&source_id, ProcessingPhase::Search)
        .expect("query search phase")
        .expect("search phase exists");
    assert_eq!(timeline.stats_json, r#"{"macbInsertedCount":4}"#);
    assert_eq!(search.state, ProcessingPhaseState::Ready);
}

#[test]
fn projection_retry_only_reruns_the_failed_timeline_action() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let timeline_calls = Cell::new(0);
    let search_calls = Cell::new(0);
    let mut first = DerivedFinalizationReport::default();

    run_phase(&runner, ProcessingPhase::Timeline, &mut first, || {
        timeline_calls.set(timeline_calls.get() + 1);
        Err("timeline writer unavailable".to_string())
    });
    run_phase(&runner, ProcessingPhase::Search, &mut first, || {
        search_calls.set(search_calls.get() + 1);
        Ok(r#"{"eligibleCount":3,"indexedCount":3,"skippedCount":0,"failedCount":0}"#.to_string())
    });

    let mut retry = DerivedFinalizationReport::default();
    run_phase(&runner, ProcessingPhase::Timeline, &mut retry, || {
        timeline_calls.set(timeline_calls.get() + 1);
        Ok(r#"{"macbInsertedCount":4}"#.to_string())
    });
    run_phase(&runner, ProcessingPhase::Search, &mut retry, || {
        search_calls.set(search_calls.get() + 1);
        Ok(r#"{"eligibleCount":99}"#.to_string())
    });

    assert_eq!(timeline_calls.get(), 2);
    assert_eq!(search_calls.get(), 1);
    let repository = DataSourceProcessingPhaseRepo::new(&conn);
    let timeline = repository
        .find(&source_id, ProcessingPhase::Timeline)
        .expect("query timeline phase")
        .expect("timeline phase exists");
    let search = repository
        .find(&source_id, ProcessingPhase::Search)
        .expect("query search phase")
        .expect("search phase exists");
    assert_eq!(timeline.state, ProcessingPhaseState::Ready);
    assert_eq!(
        search.stats_json,
        r#"{"eligibleCount":3,"indexedCount":3,"skippedCount":0,"failedCount":0}"#
    );
}

#[test]
fn timeline_counts_keep_macb_and_artifact_generated_events_separate() {
    let conn = setup_case_db();
    conn.execute_batch(
        "INSERT INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, attrs)
         VALUES
         ('macb-1', 'case-1', 'file-1', 'FILE_MODIFIED', '2026-01-01T00:00:00Z',
          'modified', '', 'timeline.macb', '{}'),
         ('artifact-1', 'case-1', 'file-1', 'LOGIN', '2026-01-01T00:00:01Z',
          'login', '', 'linux.wtmp', '{}'),
         ('artifact-2', 'case-1', 'file-2', 'SHELL_HISTORY', '2026-01-01T00:00:02Z',
          'history', '', NULL, '{}');",
    )
    .expect("insert timeline fixtures");

    assert_eq!(
        super::projections::timeline_event_counts(&conn).expect("count timeline events"),
        (1, 2)
    );
}

#[test]
fn processing_phase_fingerprints_are_phase_scoped() {
    let catalog =
        super::fingerprint::phase_input_fingerprint(FINGERPRINT, ProcessingPhase::Catalog);
    let artifacts =
        super::fingerprint::phase_input_fingerprint(FINGERPRINT, ProcessingPhase::Artifacts);
    let search = super::fingerprint::phase_input_fingerprint(FINGERPRINT, ProcessingPhase::Search);

    assert_eq!(catalog.len(), 64);
    assert_ne!(catalog, artifacts);
    assert_ne!(artifacts, search);
    assert!(catalog.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn dependency_identity_changes_when_platform_or_upstream_output_changes() {
    let linux = super::fingerprint::phase_dependency_identity(
        "artifacts",
        &[FINGERPRINT, "platform-linux"],
    );
    let windows = super::fingerprint::phase_dependency_identity(
        "artifacts",
        &[FINGERPRINT, "platform-windows"],
    );
    let changed_catalog = super::fingerprint::phase_dependency_identity(
        "artifacts",
        &[
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "platform-linux",
        ],
    );

    assert_ne!(linux, windows);
    assert_ne!(linux, changed_catalog);
}

#[test]
fn catalog_identity_includes_ready_catalog_statistics() {
    let first = catalog_identity_for_stats(r#"{"fileCount":42}"#);
    let second = catalog_identity_for_stats(r#"{"fileCount":43}"#);

    assert_eq!(first.len(), 64);
    assert_ne!(first, second);
}

#[test]
fn processing_phase_heartbeat_refreshes_a_file_backed_lease() {
    let temp = tempfile::TempDir::new().expect("create heartbeat case");
    let conn = setup_file_case_db(&temp.path().join("app.db"));
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let attempt = match runner
        .claim(ProcessingPhase::Search)
        .expect("claim search phase")
    {
        super::phase_runner::PhaseClaim::Acquired(attempt) => attempt,
        other => panic!("expected acquired search phase, got {other:?}"),
    };
    conn.execute(
        "UPDATE data_source_processing_phases
         SET heartbeat_at = '2000-01-01 00:00:00',
             lease_expires_at = '2000-01-01 00:00:01'
         WHERE data_source_id = ?1 AND phase = 'search'",
        [&source_id.0],
    )
    .expect("age heartbeat");

    let heartbeat = runner
        .start_heartbeat_with_interval(&attempt, Duration::from_millis(10))
        .expect("start heartbeat");
    std::thread::sleep(Duration::from_millis(100));
    drop(heartbeat);

    let stored = DataSourceProcessingPhaseRepo::new(&conn)
        .find(&source_id, ProcessingPhase::Search)
        .expect("query search phase")
        .expect("search phase exists");
    assert_ne!(stored.heartbeat_at.as_deref(), Some("2000-01-01 00:00:00"));
    assert_ne!(
        stored.lease_expires_at.as_deref(),
        Some("2000-01-01 00:00:01")
    );
}

fn catalog_identity_for_stats(stats_json: &str) -> String {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let attempt = match runner
        .claim(ProcessingPhase::Catalog)
        .expect("claim catalog")
    {
        super::phase_runner::PhaseClaim::Acquired(attempt) => attempt,
        other => panic!("expected acquired catalog phase, got {other:?}"),
    };
    runner
        .ready(&attempt, stats_json)
        .expect("complete catalog phase");
    super::fingerprint::load_catalog_identity(&conn, &source_id, FINGERPRINT)
        .expect("load catalog identity")
}
