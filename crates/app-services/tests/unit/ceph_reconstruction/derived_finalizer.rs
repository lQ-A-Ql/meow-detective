use std::cell::Cell;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState,
    },
    runner,
};

use super::phase_execution::{run_cancellable_phase, run_phase};
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
fn cancelled_phase_is_deferred_and_remains_retryable() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let cancel_token = AtomicBool::new(true);
    let mut report = DerivedFinalizationReport::default();

    run_cancellable_phase(
        &runner,
        ProcessingPhase::Search,
        &mut report,
        &cancel_token,
        || Err("Search indexing cancelled by user".to_string()),
    );

    let stored = DataSourceProcessingPhaseRepo::new(&conn)
        .find(&source_id, ProcessingPhase::Search)
        .expect("query search phase")
        .expect("search phase exists");
    assert_eq!(stored.state, ProcessingPhaseState::Deferred);
    assert_eq!(
        stored.stats_json,
        r#"{"reason":"userCancelled","retryable":true}"#
    );
    assert_eq!(
        stored.last_error.as_deref(),
        Some("Search indexing cancelled by user")
    );
    assert_eq!(report.deferred_count(), 1);
    assert_eq!(report.failed_count(), 0);
}

#[test]
fn retryable_artifact_read_failure_cannot_publish_a_ready_phase_payload() {
    let execution = crate::analysis_service::AnalysisExtractionExecution {
        dto: transport::dto::AnalysisExtractionRunDto {
            status: transport::dto::AnalysisParseStatusDto::Failed,
            scanned_count: 0,
            checkpoint_hit_count: 0,
            artifact_count: 0,
            timeline_event_count: 0,
            sections: Vec::new(),
            generated_at: "2026-07-18T00:00:00Z".to_string(),
            warnings: vec!["var/www/index.php read failed".to_string()],
        },
        retryable_failure_count: 1,
        discovery_elapsed_ms: 1,
        processing_elapsed_ms: 2,
        persistence_elapsed_ms: 0,
        source_read_count: 1,
        source_read_elapsed_ms: 2,
        filesystem_read_metrics: evidence_core::FileSystemReadMetrics::default(),
        rados_read_metrics: crate::ceph_reconstruction::RadosProviderReadMetrics::default(),
    };

    let error = super::artifacts::artifact_phase_output(execution)
        .expect_err("retryable read failure must keep the phase failed");
    assert!(error.contains("1 retryable evidence-read failures"));
    assert!(error.contains("var/www/index.php read failed"));
}

#[test]
fn artifact_phase_payload_records_runtime_throughput_and_memory() {
    let execution = crate::analysis_service::AnalysisExtractionExecution {
        dto: transport::dto::AnalysisExtractionRunDto {
            status: transport::dto::AnalysisParseStatusDto::Parsed,
            scanned_count: 250,
            checkpoint_hit_count: 10,
            artifact_count: 300,
            timeline_event_count: 20,
            sections: Vec::new(),
            generated_at: "2026-07-18T00:00:00Z".to_string(),
            warnings: Vec::new(),
        },
        retryable_failure_count: 0,
        discovery_elapsed_ms: 100,
        processing_elapsed_ms: 2_000,
        persistence_elapsed_ms: 50,
        source_read_count: 200,
        source_read_elapsed_ms: 1_500,
        filesystem_read_metrics: evidence_core::FileSystemReadMetrics {
            filesystem_open_operations: 1,
            metadata_cache_hits: 100,
            metadata_cache_misses: 20,
            evidence_read_operations: 60,
            evidence_bytes_read: 245_760,
        },
        rados_read_metrics: crate::ceph_reconstruction::RadosProviderReadMetrics {
            verified_cache_hits: 50,
            verified_cache_misses: 10,
            plan_cache_hits: 25,
            plan_cache_misses: 5,
            plan_lookup_elapsed_micros: 1_250,
            read_plan_session_initializations: 3,
            read_plan_session_elapsed_micros: 750,
            replica_device_reads: 30,
            replica_device_bytes: 1_966_080,
            replica_device_elapsed_micros: 25_000,
        },
    };

    let payload = super::artifacts::artifact_phase_output(execution)
        .expect("artifact phase payload should be serializable");
    let stats: serde_json::Value =
        serde_json::from_str(&payload).expect("artifact phase payload JSON");

    assert_eq!(stats["processingRowsPerSec"], 125);
    assert_eq!(stats["sourceReadAvgMicros"], 7_500);
    assert_eq!(stats["filesystemOpenOperations"], 1);
    assert_eq!(stats["filesystemMetadataCacheHits"], 100);
    assert_eq!(stats["filesystemMetadataCacheMisses"], 20);
    assert_eq!(stats["filesystemEvidenceReadOperations"], 60);
    assert_eq!(stats["filesystemEvidenceBytesRead"], 245_760);
    assert_eq!(stats["radosVerifiedCacheHits"], 50);
    assert_eq!(stats["radosVerifiedCacheMisses"], 10);
    assert_eq!(stats["radosPlanCacheHits"], 25);
    assert_eq!(stats["radosPlanCacheMisses"], 5);
    assert_eq!(stats["radosReadPlanSessionInitializations"], 3);
    assert_eq!(stats["radosReadPlanSessionElapsedMicros"], 750);
    assert_eq!(stats["radosReplicaDeviceReads"], 30);
    assert!(stats["rssMb"].is_number());
    assert!(stats["peakRssMb"].is_number());
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
fn catalog_completion_queues_the_complete_post_processing_graph() {
    let conn = setup_case_db();
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let runner = ProcessingPhaseRunner::new(&conn, &source_id, FINGERPRINT);
    let attempt = match runner
        .claim(ProcessingPhase::Catalog)
        .expect("claim catalog phase")
    {
        super::phase_runner::PhaseClaim::Acquired(attempt) => attempt,
        other => panic!("expected acquired catalog phase, got {other:?}"),
    };
    runner
        .ready(&attempt, r#"{"recordCount":42}"#)
        .expect("complete catalog phase");

    queue_post_catalog_phases(&conn, &source_id, FINGERPRINT).expect("queue post-Catalog phases");

    let phases = DataSourceProcessingPhaseRepo::new(&conn)
        .list_for_data_source(&source_id)
        .expect("list processing phases");
    assert_eq!(
        phases.iter().map(|phase| phase.phase).collect::<Vec<_>>(),
        ProcessingPhase::ALL
    );
    assert_eq!(phases[0].state, ProcessingPhaseState::Ready);
    assert!(phases[1..]
        .iter()
        .all(|phase| phase.state == ProcessingPhaseState::Pending));
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
fn timeline_counts_include_file_activity_and_analysis_events() {
    let conn = setup_case_db();
    conn.execute_batch(
        "INSERT INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, attrs)
         VALUES
         ('file-1', 'case-1', 'file-1', 'FILE_MODIFIED', '2026-01-01T00:00:00Z',
          'modified', '', 'timeline.file_modified', '{}'),
         ('file-2', 'case-1', 'file-2', 'FILE_ACCESSED', '2026-01-01T00:00:00Z',
          'accessed', '', 'timeline.file_accessed', '{}'),
         ('registry-1', 'case-1', 'file-1', 'REGISTRY_HIVE_LAST_WRITE', '2026-01-01T00:00:01Z',
          'registry', '', 'registry.hive.v1', '{}'),
         ('execution-1', 'case-1', 'file-1', 'FILE_EXECUTED', '2026-01-01T00:00:01Z',
          'executed', '', 'registry.ntuser.v1', '{}'),
         ('unsupported-1', 'case-1', 'file-2', 'SHELL_HISTORY', '2026-01-01T00:00:02Z',
          'history', '', NULL, '{}');",
    )
    .expect("insert timeline fixtures");

    assert_eq!(
        super::projections::timeline_event_counts(&conn).expect("count timeline events"),
        (2, 2)
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
fn unrelated_source_schema_migration_does_not_invalidate_catalog_fingerprint() {
    let current =
        super::fingerprint::phase_input_fingerprint(FINGERPRINT, ProcessingPhase::Catalog);
    let source_015 = super::fingerprint::phase_input_fingerprint_with_contract(
        FINGERPRINT,
        ProcessingPhase::Catalog,
        "source_015_ceph_bluestore_rbd_header_context",
        super::fingerprint::CATALOG_POLICY_VERSION,
    );
    let latest_source = super::fingerprint::phase_input_fingerprint_with_contract(
        FINGERPRINT,
        ProcessingPhase::Catalog,
        persistence_sqlite::runner::latest_source_version(),
        super::fingerprint::CATALOG_POLICY_VERSION,
    );

    assert_eq!(
        persistence_sqlite::runner::latest_source_version(),
        "source_029_case_graph_entity_index"
    );
    assert_eq!(
        super::fingerprint::phase_schema_dependency(ProcessingPhase::Catalog),
        "source_015_ceph_bluestore_rbd_header_context"
    );
    assert_eq!(
        super::fingerprint::phase_schema_dependency(ProcessingPhase::Artifacts),
        "source_016_file_partition_index"
    );
    assert_eq!(current, source_015);
    assert_ne!(current, latest_source);
}

#[test]
fn catalog_policy_change_invalidates_catalog_fingerprint() {
    let current =
        super::fingerprint::phase_input_fingerprint(FINGERPRINT, ProcessingPhase::Catalog);
    let changed = super::fingerprint::phase_input_fingerprint_with_contract(
        FINGERPRINT,
        ProcessingPhase::Catalog,
        super::fingerprint::phase_schema_dependency(ProcessingPhase::Catalog),
        "rbd-filesystem-catalog-v3",
    );

    assert_ne!(current, changed);
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
