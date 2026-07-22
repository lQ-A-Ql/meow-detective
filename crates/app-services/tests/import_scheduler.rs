use app_services::import_scheduler::{
    resolve_analysis_worker_count_for_memory, ImportAdmission, ImportAdmissionRequest,
    ImportSchedulingPolicy, ImportWorkload,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn ordinary_and_cluster_policies_share_the_same_cpu_cap() {
    let ordinary = ImportSchedulingPolicy::for_workload(ImportWorkload::SingleSource, None, None);
    let cluster = ImportSchedulingPolicy::for_workload(
        ImportWorkload::LinuxCluster { member_count: 6 },
        None,
        None,
    );

    assert!((1..=6).contains(&ordinary.cpu_budget));
    assert!(ordinary.import_workers <= ordinary.cpu_budget);
    assert!(ordinary.analysis_workers <= ordinary.cpu_budget);
    assert!(cluster.source_concurrency <= 2);
    assert!(cluster.import_workers <= 3);
    assert!(cluster.analysis_workers <= 3);
    assert!(
        cluster.admission_request().cpu_weight * cluster.source_concurrency <= cluster.cpu_budget
    );
}

#[test]
fn cluster_policy_keeps_two_low_weight_members_parallel() {
    let policy = ImportSchedulingPolicy::for_linux_cluster(4, 1, 1, 6);

    assert_eq!(policy.source_concurrency, 2);
    assert_eq!(policy.source_worker_count(100), 2);
    assert_eq!(policy.source_worker_count(0), 0);
    assert_eq!(policy.import_workers, 1);
    assert_eq!(policy.analysis_workers, 1);
    assert_eq!(policy.memory_reservation_mb, 2048);
}

#[test]
fn cluster_policy_uses_three_workers_per_member_with_six_cpu_budget() {
    let policy = ImportSchedulingPolicy::for_linux_cluster(6, 99, 99, 6);

    assert_eq!(policy.import_workers, 3);
    assert_eq!(policy.analysis_workers, 3);
    assert_eq!(policy.source_concurrency, 2);
    assert_eq!(policy.admission_request().cpu_weight, 3);
    assert_eq!(
        policy.admission_request().cpu_weight * policy.source_concurrency,
        6
    );
    assert_eq!(policy.memory_reservation_mb, 2048);
}

#[test]
fn cluster_policy_respects_a_small_cpu_budget() {
    let policy = ImportSchedulingPolicy::for_linux_cluster(1, 6, 6, 6);

    assert_eq!(policy.import_workers, 1);
    assert_eq!(policy.analysis_workers, 1);
    assert_eq!(policy.source_concurrency, 1);
    assert!(policy.admission_request().cpu_weight * policy.source_concurrency <= policy.cpu_budget);
}

#[test]
fn analysis_worker_budget_reduces_with_remaining_memory_headroom() {
    assert_eq!(
        resolve_analysis_worker_count_for_memory(Some(6), 0, 4096),
        6
    );
    assert_eq!(
        resolve_analysis_worker_count_for_memory(Some(6), 1024, 4096),
        6
    );
    assert_eq!(
        resolve_analysis_worker_count_for_memory(Some(6), 2048, 4096),
        4
    );
    assert_eq!(
        resolve_analysis_worker_count_for_memory(Some(6), 3584, 4096),
        1
    );
    assert_eq!(
        resolve_analysis_worker_count_for_memory(Some(6), 6144, 4096),
        1
    );
}

#[test]
fn admission_bounds_active_weight_and_releases_permits() {
    let admission = ImportAdmission::new(4, 4096);
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let admission = admission.clone();
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        handles.push(thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let _permit = admission
                .acquire(
                    ImportAdmissionRequest {
                        cpu_weight: 2,
                        memory_mb: 2048,
                    },
                    &cancel,
                )
                .expect("admission should eventually succeed");
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            active.fetch_sub(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.join().expect("admission worker should not panic");
    }
    assert!(maximum.load(Ordering::SeqCst) <= 2);
    assert_eq!(admission.snapshot().cpu_in_use, 0);
    assert_eq!(admission.snapshot().memory_in_use_mb, 0);
    assert_eq!(admission.snapshot().active_sources, 0);
    assert_eq!(admission.snapshot().peak_active_sources, 2);
    assert_eq!(admission.snapshot().peak_cpu_in_use, 4);
    assert_eq!(admission.snapshot().peak_memory_in_use_mb, 4096);
}

#[test]
fn admission_wait_is_cancelable() {
    let admission = ImportAdmission::new(1, 1024);
    let cancel = AtomicBool::new(false);
    let first = admission
        .acquire(
            ImportAdmissionRequest {
                cpu_weight: 1,
                memory_mb: 1024,
            },
            &cancel,
        )
        .expect("first permit should be admitted");
    let waiting_cancel = Arc::new(AtomicBool::new(false));
    let waiting_cancel_for_thread = Arc::clone(&waiting_cancel);
    let admission_for_thread = admission.clone();
    let waiter = thread::spawn(move || {
        admission_for_thread.acquire(
            ImportAdmissionRequest {
                cpu_weight: 1,
                memory_mb: 1024,
            },
            &waiting_cancel_for_thread,
        )
    });
    thread::sleep(Duration::from_millis(20));
    waiting_cancel.store(true, Ordering::Release);
    assert!(waiter.join().expect("waiter should not panic").is_err());
    drop(first);
}

#[test]
fn cancelling_multiple_waiters_drains_without_leaking_capacity() {
    let admission = ImportAdmission::new(1, 1024);
    let owner_cancel = AtomicBool::new(false);
    let owner = admission
        .acquire(
            ImportAdmissionRequest {
                cpu_weight: 1,
                memory_mb: 1024,
            },
            &owner_cancel,
        )
        .expect("owner permit");
    let cancel = Arc::new(AtomicBool::new(false));
    let waiters = (0..3)
        .map(|_| {
            let admission = admission.clone();
            let cancel = Arc::clone(&cancel);
            thread::spawn(move || {
                admission.acquire(
                    ImportAdmissionRequest {
                        cpu_weight: 1,
                        memory_mb: 1024,
                    },
                    &cancel,
                )
            })
        })
        .collect::<Vec<_>>();

    thread::sleep(Duration::from_millis(20));
    cancel.store(true, Ordering::Release);
    for waiter in waiters {
        assert!(waiter.join().expect("waiter should not panic").is_err());
    }
    drop(owner);
    let snapshot = admission.snapshot();
    assert_eq!(snapshot.active_sources, 0);
    assert_eq!(snapshot.cpu_in_use, 0);
    assert_eq!(snapshot.memory_in_use_mb, 0);
}

#[test]
fn permit_releases_capacity_during_panic_unwind() {
    let admission = ImportAdmission::new(1, 1024);
    let admission_for_thread = admission.clone();
    let worker = thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let _permit = admission_for_thread
            .acquire(
                ImportAdmissionRequest {
                    cpu_weight: 1,
                    memory_mb: 1024,
                },
                &cancel,
            )
            .expect("permit before panic");
        panic!("synthetic worker failure");
    });

    assert!(worker.join().is_err());
    let snapshot = admission.snapshot();
    assert_eq!(snapshot.active_sources, 0);
    assert_eq!(snapshot.cpu_in_use, 0);
    assert_eq!(snapshot.memory_in_use_mb, 0);
}
