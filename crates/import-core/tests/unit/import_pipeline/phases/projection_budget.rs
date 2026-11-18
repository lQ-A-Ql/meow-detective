use super::*;
use crate::timeline_projection::{self, TimelineProjectionError};
use chrono::{TimeZone, Utc};
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance, EntryType,
    FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::timeline_projection_repo::{
    TimelineProjectionMetaState, TimelineProjectionRepo,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};
use transport::{CommandError, ServiceErrorCategory};

crate::native_source_test_support!();
use test_support::NativeSourceFixture;

fn default_policy() -> TimelineProjectionPolicy {
    TimelineProjectionPolicy::default()
}

fn policy_128mib() -> TimelineProjectionPolicy {
    TimelineProjectionPolicy {
        workspace_kib: 128 * 1024,
    }
}

fn policy_384mib() -> TimelineProjectionPolicy {
    TimelineProjectionPolicy {
        workspace_kib: 384 * 1024,
    }
}

fn pool(limit_bytes: u64) -> ResourcePool {
    ResourcePool::new(Resources {
        bytes: limit_bytes,
        ..Default::default()
    })
}

fn fixture(count: usize) -> NativeSourceFixture {
    let fixture = NativeSourceFixture::new("projection-source", "projection-case");
    for start in (0..count).step_by(1024) {
        let rows = (start..(start + 1024).min(count))
            .map(|index| FileEntry {
                id: FileEntryId(format!("projection-{index}")),
                parent_id: None,
                data_source_id: DataSourceId(fixture.source_id.clone()),
                path: format!("documents/file-{index}.txt"),
                name: format!("file-{index}.txt"),
                entry_type: EntryType::File,
                size: Some(8),
                ext: Some("txt".into()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: Some(
                    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
                        + chrono::Duration::seconds(index as i64),
                ),
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            })
            .collect::<Vec<_>>();
        persistence_source::repositories::file_repo::FileRepo::new(&fixture.connection)
            .insert_batch(&rows)
            .unwrap();
    }
    fixture.seal();
    fixture
}

fn project(
    fixture: &NativeSourceFixture,
    policy: TimelineProjectionPolicy,
    cancel: &AtomicBool,
) -> Result<crate::timeline_projection::TimelineProjectionStats, TimelineProjectionError> {
    timeline_projection::materialize_file_activity_with_policy(
        &fixture.connection,
        DataSourcePlatform::Unknown,
        cancel,
        policy,
    )
}

#[test]
fn constant_plan_passes_default_policy_and_fails_128mib() {
    let cancel = AtomicBool::new(false);
    let estimate = crate::timeline_projection::workspace::estimate(&cancel).unwrap();
    estimate.require(default_policy()).unwrap();
    let error = estimate.require(policy_128mib()).unwrap_err();
    assert_eq!(error.code(), Some("TIMELINE_WORKSPACE_LIMIT"));
    let TimelineProjectionError::WorkspaceLimit {
        required_bytes,
        available_bytes,
        ..
    } = error
    else {
        panic!("expected WorkspaceLimit")
    };
    assert_eq!(required_bytes, timeline_projection::plan_required_bytes());
    assert_eq!(
        available_bytes,
        policy_128mib().reservation_bytes().unwrap()
    );
}

#[test]
fn estimate_is_decoupled_from_catalog_size() {
    let _small = fixture(16);
    let _large = fixture(20_000);
    let cancel = AtomicBool::new(false);
    let first = crate::timeline_projection::workspace::estimate(&cancel).unwrap();
    let second = crate::timeline_projection::workspace::estimate(&cancel).unwrap();
    assert_eq!(first.required_bytes, second.required_bytes);
    assert_eq!(
        first.required_bytes,
        timeline_projection::plan_required_bytes()
    );
    assert_eq!(
        first.engine_retained_bytes,
        160 * 1024 * 1024,
        "engine retained stays at the calibrated constant pending measurement"
    );
}

#[test]
fn reservation_covers_the_plan_and_releases_after_success() {
    let policy = default_policy();
    let pool = pool(policy.reservation_bytes().unwrap());
    let fixture = fixture(16);
    let cancel = AtomicBool::new(false);
    let planned = timeline_projection::plan_required_bytes();
    let result = run(&pool, policy, planned, &cancel, || {
        assert_eq!(pool.snapshot().0.bytes, planned);
        project(&fixture, policy, &cancel)
    })
    .unwrap();
    assert_eq!(result.inserted_count, 16);
    assert!(result.graph_complete);
    assert!(result.workspace_segments >= 1);
    assert_eq!(result.commit_count, result.workspace_segments);
    assert!(result.workspace_max_segment_bytes > 0);
    assert_eq!(result.workspace_engine_retained_bytes, 160 * 1024 * 1024);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn invalid_policy_fails_before_admission() {
    let pool = pool(default_policy().reservation_bytes().unwrap());
    let fixture = fixture(1);
    let invalid = TimelineProjectionPolicy { workspace_kib: 0 };
    let cancel = AtomicBool::new(false);
    let error = CommandError::from_typed_service_error(
        run(
            &pool,
            invalid,
            timeline_projection::plan_required_bytes(),
            &cancel,
            || project(&fixture, invalid, &cancel),
        )
        .unwrap_err(),
    );
    assert_eq!(error.category, "validation");
    assert_eq!(pool.snapshot().1, Resources::default());
}

#[test]
fn real_projection_failure_releases_admission() {
    let policy = default_policy();
    let pool = pool(policy.reservation_bytes().unwrap());
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let cancel = AtomicBool::new(false);
    let result = run(
        &pool,
        policy,
        timeline_projection::plan_required_bytes(),
        &cancel,
        || {
            timeline_projection::materialize_file_activity_with_policy(
                &connection,
                DataSourcePlatform::Unknown,
                &cancel,
                policy,
            )
        },
    );
    assert!(result.is_err());
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn pool_limit_maps_to_workspace_limit_with_the_same_code() {
    let policy = default_policy();
    let planned = timeline_projection::plan_required_bytes();
    let pool = pool(planned - 1);
    let fixture = fixture(1);
    let cancel = AtomicBool::new(false);
    let error = CommandError::from_typed_service_error(
        run(&pool, policy, planned, &cancel, || {
            project(&fixture, policy, &cancel)
        })
        .unwrap_err(),
    );
    assert_eq!(error.code, "TIMELINE_WORKSPACE_LIMIT");
    assert_eq!(error.category, "validation");
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn cancellation_before_admission_is_propagated_not_deferred() {
    let policy = default_policy();
    let pool = pool(policy.reservation_bytes().unwrap());
    let fixture = fixture(1);
    let cancel = AtomicBool::new(true);
    let error = CommandError::from_typed_service_error(
        run(
            &pool,
            policy,
            timeline_projection::plan_required_bytes(),
            &cancel,
            || project(&fixture, policy, &cancel),
        )
        .unwrap_err(),
    );
    assert_eq!(error.category, "cancelled");
    assert_ne!(error.code, "TIMELINE_WORKSPACE_LIMIT");
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn waiting_projection_cancellation_preserves_the_existing_reservation() {
    let policy = default_policy();
    let pool = pool(policy.reservation_bytes().unwrap());
    let owner = pool
        .acquire(
            Resources {
                bytes: policy.reservation_bytes().unwrap(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let fixture = fixture(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_pool = pool.clone();
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run(
            &worker_pool,
            policy,
            timeline_projection::plan_required_bytes(),
            &worker_cancel,
            || project(&fixture, policy, &worker_cancel),
        );
        tx.send(result.map(|stats| stats.inserted_count)).unwrap();
    });
    assert!(matches!(
        rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    cancel.store(true, Ordering::Release);
    let error = rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err();
    assert_eq!(error.category().as_str(), "cancelled");
    worker.join().unwrap();
    assert_eq!(pool.snapshot().0.bytes, policy.reservation_bytes().unwrap());
    drop(owner);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn concurrent_real_projections_obey_workspace_admission() {
    let policy = default_policy();
    let planned = timeline_projection::plan_required_bytes();
    let pool = pool(planned);
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let fixtures = [fixture(32), fixture(32)];
    std::thread::scope(|scope| {
        let handles = fixtures
            .into_iter()
            .map(|fixture| {
                let pool = pool.clone();
                let active = active.clone();
                let maximum = maximum.clone();
                scope.spawn(move || {
                    let cancel = AtomicBool::new(false);
                    run(
                        &pool,
                        policy,
                        timeline_projection::plan_required_bytes(),
                        &cancel,
                        || {
                            let count = active.fetch_add(1, Ordering::AcqRel) + 1;
                            maximum.fetch_max(count, Ordering::AcqRel);
                            let result = project(&fixture, policy, &cancel);
                            active.fetch_sub(1, Ordering::AcqRel);
                            result
                        },
                    )
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(handle.join().unwrap().inserted_count, 32);
        }
    });
    assert_eq!(maximum.load(Ordering::Acquire), 1);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn large_input_rejects_insufficient_workspace_before_projection_writes() {
    let policy = policy_128mib();
    let pool = pool(policy.reservation_bytes().unwrap());
    let fixture = fixture(20_000);
    let cancel = AtomicBool::new(false);
    let error = CommandError::from_typed_service_error(
        run(
            &pool,
            policy,
            timeline_projection::plan_required_bytes(),
            &cancel,
            || project(&fixture, policy, &cancel),
        )
        .unwrap_err(),
    );
    assert_eq!(error.code, "TIMELINE_WORKSPACE_LIMIT");
    assert_eq!(error.category, "validation");
    let details = error.details.as_ref().unwrap();
    assert_eq!(
        details["requiredBytes"].as_u64().unwrap(),
        timeline_projection::plan_required_bytes()
    );
    assert_eq!(
        details["availableBytes"].as_u64().unwrap(),
        policy.reservation_bytes().unwrap()
    );
    let store = persistence_source::detail::open_store(&fixture.connection)
        .unwrap()
        .unwrap();
    let timeline =
        persistence_source::fjall::TimelineRecordStore::from_source_store((*store).clone());
    assert!(timeline
        .list_by_source(
            &timeline.read_snapshot(),
            &fixture.source_id,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            1,
        )
        .unwrap()
        .records
        .is_empty());
    let graph_rows: i64 = fixture
        .connection
        .query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(graph_rows, 0);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn sufficient_workspace_executes_large_input_and_completed_input_needs_no_rewrite() {
    let policy = policy_384mib();
    let pool = pool(policy.reservation_bytes().unwrap());
    let fixture = fixture(20_000);
    let cancel = AtomicBool::new(false);
    let result = run(
        &pool,
        policy,
        timeline_projection::plan_required_bytes(),
        &cancel,
        || project(&fixture, policy, &cancel),
    )
    .unwrap();
    assert_eq!(result.inserted_count, 20_000);
    assert!(result.workspace_required_bytes <= result.workspace_available_bytes);
    assert!(result.workspace_required_bytes > 128 * 1024 * 1024);
    assert!(result.workspace_segments >= 1);
    assert_eq!(result.commit_count, result.workspace_segments);
    assert_eq!(pool.snapshot().0, Resources::default());
    let lower = policy_128mib();
    let repeated = run(
        &pool,
        lower,
        timeline_projection::plan_required_bytes(),
        &cancel,
        || project(&fixture, lower, &cancel),
    )
    .unwrap();
    assert!(repeated.already_projected);
    assert_eq!(repeated.inserted_count, 0);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn deferred_marker_writes_sqlite_row_and_fjall_deferred_marker() {
    let fixture = fixture(1);
    let data_source = DataSource {
        id: DataSourceId(fixture.source_id.clone()),
        name: "projection-source".into(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: Default::default(),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let required = timeline_projection::plan_required_bytes();
    let available = policy_128mib().reservation_bytes().unwrap();
    super::super::write_deferred_marker(
        &fixture.connection,
        &data_source,
        "identity-deferred",
        required,
        available,
        "capacity",
    )
    .unwrap();
    let meta = TimelineProjectionRepo::new(&fixture.connection);
    let status = meta.status("file_activity_v2").unwrap().unwrap();
    assert_eq!(status.state, TimelineProjectionMetaState::Deferred);
    assert_eq!(status.input_identity, "identity-deferred");
    assert_eq!(status.required_bytes, required);
    assert_eq!(status.available_bytes, available);
    assert_eq!(status.deferred_reason.as_deref(), Some("capacity"));

    let store = persistence_source::detail::open_store(&fixture.connection)
        .unwrap()
        .unwrap();
    let snapshot = store.database().read_snapshot();
    let key = persistence_source::fjall::timeline_projection_state_key(&fixture.source_id).unwrap();
    let envelope = store
        .read_envelope(
            &snapshot,
            persistence_source::fjall::KeyspaceName::Meta,
            &key,
        )
        .unwrap()
        .unwrap();
    let state = persistence_source::fjall::decode_timeline_projection_state(&envelope)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.status,
        persistence_source::fjall::TimelineProjectionState::Deferred
    );
    assert_eq!(state.identity, "identity-deferred");
    assert_eq!(state.required_bytes, required);
    assert_eq!(state.available_bytes, available);
}

#[test]
#[ignore = "explicit projection workspace RSS measurement on a 50K catalog"]
fn projection_workspace_memory_measurement() {
    let started = Instant::now();
    eprintln!(
        "[PROJECTION-WORKSPACE-PHASE] phase=prepare-start rows=50000 elapsedMs={}",
        started.elapsed().as_millis()
    );
    let policy = default_policy();
    let workspace_kib = policy.workspace_kib;
    let pool = pool(policy.reservation_bytes().unwrap());
    let fixture = fixture(50_000);
    eprintln!(
        "[PROJECTION-WORKSPACE-PHASE] phase=prepare-complete rows=50000 elapsedMs={}",
        started.elapsed().as_millis()
    );
    let before = source_kernel::runtime_resources::current_rss_mb();
    let stop = Arc::new(AtomicBool::new(false));
    let sampled_stop = stop.clone();
    let sampler = std::thread::spawn(move || {
        let mut peak = before;
        while !sampled_stop.load(Ordering::Acquire) {
            peak = peak.max(source_kernel::runtime_resources::current_rss_mb());
            std::thread::sleep(Duration::from_millis(2));
        }
        peak.max(source_kernel::runtime_resources::current_rss_mb())
    });
    let cancel = AtomicBool::new(false);
    eprintln!(
        "[PROJECTION-WORKSPACE-PHASE] phase=projection-start elapsedMs={}",
        started.elapsed().as_millis()
    );
    let started = Instant::now();
    let result = run(
        &pool,
        policy,
        timeline_projection::plan_required_bytes(),
        &cancel,
        || project(&fixture, policy, &cancel),
    );
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Release);
    let peak = sampler.join().unwrap();
    let result = result.unwrap();
    assert_eq!(result.inserted_count, 50_000);
    assert_eq!(pool.snapshot().0, Resources::default());
    assert!(result.workspace_required_bytes <= result.workspace_available_bytes);
    eprintln!(
        "[PROJECTION-WORKSPACE-PHASE] phase=projection-complete elapsedMs={}",
        elapsed.as_millis()
    );
    eprintln!(
        "[PROJECTION-WORKSPACE] {}",
        serde_json::json!({
            "rows": 50_000, "workspaceKiB": workspace_kib, "reservationBytes": policy.reservation_bytes().unwrap(),
            "requiredBytes": result.workspace_required_bytes, "availableBytes": result.workspace_available_bytes,
            "engineRetainedBytes": result.workspace_engine_retained_bytes,
            "segments": result.workspace_segments, "commitCount": result.commit_count,
            "maxSegmentBytes": result.workspace_max_segment_bytes,
            "budgetModel": "timeline-workspace-constant-v2",
            "rssAtStartMiB": before, "sampledPeakRssMiB": peak, "sampleIntervalMs": 2,
            "processLifetimePeakRssMiB": source_kernel::runtime_resources::peak_rss_mb(),
            "elapsedMs": elapsed.as_millis(), "reservedBytesAfter": pool.snapshot().0.bytes
        })
    );
}
