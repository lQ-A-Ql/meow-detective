use super::*;
use crate::timeline_projection::materialize_file_activity_with_policy;
use chrono::{TimeZone, Utc};
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

crate::native_source_test_support!();
use test_support::NativeSourceFixture;

fn policy() -> TimelineProjectionPolicy {
    TimelineProjectionPolicy {
        workspace_kib: 128 * 1024,
    }
}

fn pool(policy: TimelineProjectionPolicy) -> ResourcePool {
    ResourcePool::new(Resources {
        bytes: policy.reservation_bytes().unwrap(),
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
) -> Result<crate::timeline_projection::TimelineProjectionStats, CommandError> {
    materialize_file_activity_with_policy(
        &fixture.connection,
        DataSourcePlatform::Unknown,
        cancel,
        policy,
    )
    .map_err(CommandError::from_typed_service_error)
}

#[test]
fn reservation_covers_the_real_projection_and_releases_after_success() {
    let policy = policy();
    let pool = pool(policy);
    let fixture = fixture(16);
    let cancel = AtomicBool::new(false);
    let result = run(&pool, policy, &cancel, || {
        assert_eq!(pool.snapshot().0.bytes, policy.reservation_bytes().unwrap());
        project(&fixture, policy, &cancel)
    })
    .unwrap();
    assert_eq!(result.inserted_count, 16);
    assert!(result.graph_complete);
    assert_eq!(pool.snapshot().0, Resources::default());
    assert_eq!(pool.snapshot().1.bytes, policy.reservation_bytes().unwrap());
}

#[test]
fn invalid_policy_fails_before_admission() {
    let pool = pool(policy());
    let fixture = fixture(1);
    let invalid = TimelineProjectionPolicy { workspace_kib: 0 };
    let cancel = AtomicBool::new(false);
    let error = run(&pool, invalid, &cancel, || {
        project(&fixture, invalid, &cancel)
    })
    .unwrap_err();
    assert_eq!(error.category, "validation");
    assert_eq!(pool.snapshot().1, Resources::default());
}

#[test]
fn real_projection_failure_releases_admission() {
    let policy = policy();
    let pool = pool(policy);
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let cancel = AtomicBool::new(false);
    let result = run(&pool, policy, &cancel, || {
        materialize_file_activity_with_policy(
            &connection,
            DataSourcePlatform::Unknown,
            &cancel,
            policy,
        )
        .map_err(CommandError::from_typed_service_error)
    });
    assert!(result.is_err());
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn waiting_projection_cancellation_preserves_the_existing_reservation() {
    let policy = policy();
    let pool = pool(policy);
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
        let result = run(&worker_pool, policy, &worker_cancel, || {
            project(&fixture, policy, &worker_cancel)
        });
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
    assert_eq!(error.category, "cancelled");
    worker.join().unwrap();
    assert_eq!(pool.snapshot().0.bytes, policy.reservation_bytes().unwrap());
    drop(owner);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn concurrent_real_projections_obey_workspace_admission() {
    let policy = policy();
    let pool = pool(policy);
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
                    run(&pool, policy, &cancel, || {
                        let count = active.fetch_add(1, Ordering::AcqRel) + 1;
                        maximum.fetch_max(count, Ordering::AcqRel);
                        let result = project(&fixture, policy, &cancel);
                        active.fetch_sub(1, Ordering::AcqRel);
                        result
                    })
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
#[ignore = "explicit projection workspace RSS measurement on a 50K catalog"]
fn projection_workspace_memory_measurement() {
    for workspace_kib in [128 * 1024, 256 * 1024, 1_000_000] {
        let policy = TimelineProjectionPolicy { workspace_kib };
        let pool = pool(policy);
        let fixture = fixture(50_000);
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
        let started = Instant::now();
        let result = run(&pool, policy, &cancel, || {
            project(&fixture, policy, &cancel)
        });
        let elapsed = started.elapsed();
        stop.store(true, Ordering::Release);
        let peak = sampler.join().unwrap();
        let result = result.unwrap();
        assert_eq!(result.inserted_count, 50_000);
        assert_eq!(pool.snapshot().0, Resources::default());
        eprintln!(
            "[PROJECTION-WORKSPACE] {}",
            serde_json::json!({
                "rows": 50_000, "workspaceKiB": workspace_kib, "reservationBytes": policy.reservation_bytes().unwrap(),
                "rssAtStartMiB": before, "sampledPeakRssMiB": peak, "sampleIntervalMs": 2,
                "processLifetimePeakRssMiB": source_kernel::runtime_resources::peak_rss_mb(),
                "elapsedMs": elapsed.as_millis(), "reservedBytesAfter": pool.snapshot().0.bytes
            })
        );
    }
}
