use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

use super::{global_import_group_gate, ImportGroupGate, ImportGroupGateError};

fn wait_until_queued(gate: &ImportGroupGate, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if gate.inner.state.lock().unwrap().queue.len() == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "gate queue did not reach {expected} waiter(s)"
        );
        thread::yield_now();
    }
}

#[test]
fn gate_releases_next_waiter_and_remains_process_global() {
    let first = global_import_group_gate()
        .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .unwrap();
    let Err(error) = global_import_group_gate().acquire(
        "case-a",
        &AtomicBool::new(false),
        Duration::from_millis(20),
    ) else {
        panic!("global gate should reject the second holder");
    };
    assert_eq!(error, ImportGroupGateError::TimedOut);
    drop(first);
    global_import_group_gate()
        .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .unwrap();
}

#[test]
fn gate_honours_cancellation_before_grant() {
    let gate = ImportGroupGate::new();
    let cancel = Arc::new(AtomicBool::new(true));
    let Err(error) = gate.acquire("case-a", cancel.as_ref(), Duration::from_secs(1)) else {
        panic!("pre-cancelled waiter should not acquire the gate");
    };
    assert_eq!(error, ImportGroupGateError::Cancelled);
}

#[test]
fn gate_honours_cancellation_while_waiting() {
    let gate = ImportGroupGate::new();
    let owner = gate
        .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .expect("owner should acquire the gate");
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_waiter = Arc::clone(&cancel);
    let gate_for_waiter = gate.clone();
    let waiter = thread::spawn(move || {
        gate_for_waiter.acquire("case-a", &cancel_for_waiter, Duration::from_secs(1))
    });
    wait_until_queued(&gate, 1);

    cancel.store(true, Ordering::Release);

    let Err(error) = waiter.join().expect("waiter should not panic") else {
        panic!("cancelled waiter should not acquire the gate");
    };
    assert_eq!(error, ImportGroupGateError::Cancelled);
    assert_eq!(gate.inner.state.lock().unwrap().queue.len(), 0);
    drop(owner);
    gate.acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .expect("cancelled waiter must not keep the gate occupied");
}

#[test]
fn gate_grants_waiters_in_arrival_order() {
    let gate = ImportGroupGate::new();
    let owner = gate
        .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .expect("owner should acquire the gate");
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first_gate = gate.clone();
    let first_acquired = acquired_tx.clone();
    let first = thread::spawn(move || {
        let _lease = first_gate
            .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
            .expect("first waiter should acquire the gate");
        first_acquired.send(1).unwrap();
        release_rx.recv().unwrap();
    });
    wait_until_queued(&gate, 1);

    let second_gate = gate.clone();
    let second = thread::spawn(move || {
        let _lease = second_gate
            .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
            .expect("second waiter should acquire the gate");
        acquired_tx.send(2).unwrap();
    });
    wait_until_queued(&gate, 2);
    drop(owner);

    assert_eq!(acquired_rx.recv().unwrap(), 1);
    assert!(acquired_rx.try_recv().is_err());
    release_tx.send(()).unwrap();
    assert_eq!(acquired_rx.recv().unwrap(), 2);
    first.join().expect("first waiter should not panic");
    second.join().expect("second waiter should not panic");
}

#[test]
fn gate_allows_independent_case_groups_to_run_concurrently() {
    let gate = ImportGroupGate::new();
    let first = gate
        .acquire("case-a", &AtomicBool::new(false), Duration::from_secs(1))
        .expect("first group should acquire");
    let second = gate
        .acquire("case-b", &AtomicBool::new(false), Duration::from_secs(1))
        .expect("independent group should acquire");
    drop(second);
    drop(first);
}
