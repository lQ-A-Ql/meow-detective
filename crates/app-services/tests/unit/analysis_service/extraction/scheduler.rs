use super::gate::ExtractionGate;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[test]
fn bounded_scheduler_runs_work_in_parallel_and_applies_in_input_order() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let prepared_thread = std::thread::current().id();
    let applied = Mutex::new(Vec::new());
    let active_for_work = Arc::clone(&active);
    let peak_for_work = Arc::clone(&peak);

    run_bounded_ordered(
        0usize..12,
        ExtractionSchedulingPolicy {
            worker_count: 3,
            max_in_flight_items: 3,
            max_in_flight_bytes: 3,
        },
        &mut (),
        |_| 1,
        |_, value| {
            assert_eq!(std::thread::current().id(), prepared_thread);
            Ok::<_, String>(PreparedWork::Parallel {
                input: value,
                weight_bytes: 1,
            })
        },
        move |value| {
            let current = active_for_work.fetch_add(1, Ordering::SeqCst) + 1;
            peak_for_work.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(
                ((12 - value) % 3 + 1) as u64 * 4,
            ));
            active_for_work.fetch_sub(1, Ordering::SeqCst);
            value
        },
        |_, value| {
            applied.lock().expect("applied lock").push(value);
            Ok(())
        },
        |sequence, message| Some(format!("worker {sequence}: {message}")),
        |_| {},
    )
    .expect("run bounded scheduler");

    assert_eq!(
        *applied.lock().expect("applied lock"),
        (0usize..12).collect::<Vec<_>>()
    );
    assert!(peak.load(Ordering::SeqCst) > 1);
    assert!(peak.load(Ordering::SeqCst) <= 3);
}

#[test]
fn scheduling_policy_throttles_workers_at_the_memory_soft_limit() {
    let normal = ExtractionSchedulingPolicy::for_runtime(6, 1024, 4096);
    assert_eq!(normal.worker_count, 6);
    assert_eq!(normal.max_in_flight_items, 6);
    assert_eq!(normal.max_in_flight_bytes, DEFAULT_MAX_IN_FLIGHT_BYTES);

    let pressure = ExtractionSchedulingPolicy::for_runtime(6, 2048, 4096);
    assert_eq!(pressure.worker_count, 4);
    assert_eq!(pressure.max_in_flight_items, 4);

    let throttled = ExtractionSchedulingPolicy::for_runtime(6, 4096, 4096);
    assert_eq!(throttled.worker_count, 1);
    assert_eq!(throttled.max_in_flight_items, 1);
    assert_eq!(throttled.max_in_flight_bytes, THROTTLED_MAX_IN_FLIGHT_BYTES);
}

#[test]
fn scheduler_turns_worker_panics_into_typed_errors() {
    let error = run_bounded_ordered(
        [7usize],
        ExtractionSchedulingPolicy {
            worker_count: 1,
            max_in_flight_items: 1,
            max_in_flight_bytes: 1,
        },
        &mut (),
        |_| 1,
        |_, value| {
            Ok::<_, String>(PreparedWork::Parallel {
                input: value,
                weight_bytes: 1,
            })
        },
        |_| -> usize { panic!("injected parser panic") },
        |_, _| Ok(()),
        |sequence, message| Some(format!("worker {sequence}: {message}")),
        |_| {},
    )
    .expect_err("worker panic must become an error");

    assert!(error.contains("worker 0"));
    assert!(error.contains("injected parser panic"));
}

#[test]
fn scheduler_degrades_worker_panics_into_skipped_candidates() {
    let diagnostics = Mutex::new(Vec::new());
    let applied = Mutex::new(Vec::new());

    run_bounded_ordered(
        0usize..6,
        ExtractionSchedulingPolicy {
            worker_count: 2,
            max_in_flight_items: 2,
            max_in_flight_bytes: 2,
        },
        &mut (),
        |_| 1,
        |_, value| {
            Ok::<_, String>(PreparedWork::Parallel {
                input: value,
                weight_bytes: 1,
            })
        },
        |value| -> usize {
            if value == 2 {
                panic!("injected parser panic");
            }
            value
        },
        |_, value| {
            applied.lock().expect("applied lock").push(value);
            Ok(())
        },
        |sequence, message| {
            diagnostics
                .lock()
                .expect("diagnostics lock")
                .push(format!("candidate {sequence}: {message}"));
            None::<String>
        },
        |_| {},
    )
    .expect("a degraded run must succeed despite the panicking candidate");

    assert_eq!(
        *applied.lock().expect("applied lock"),
        vec![0, 1, 3, 4, 5],
        "the panicked candidate yields no output but the ordered merge continues"
    );
    let diagnostics = diagnostics.lock().expect("diagnostics lock");
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("candidate 2"));
    assert!(diagnostics[0].contains("injected parser panic"));
}

#[test]
fn scheduler_applies_ready_results_past_a_degraded_panic() {
    let applied = Mutex::new(Vec::new());

    run_bounded_ordered(
        0usize..4,
        ExtractionSchedulingPolicy {
            worker_count: 1,
            max_in_flight_items: 4,
            max_in_flight_bytes: 4,
        },
        &mut (),
        |_| 1,
        |_, value| {
            if value % 2 == 0 {
                return Ok::<_, String>(PreparedWork::Ready(value));
            }
            Ok(PreparedWork::Parallel {
                input: value,
                weight_bytes: 1,
            })
        },
        |value| -> usize {
            if value == 1 {
                panic!("injected parser panic");
            }
            value
        },
        |_, value| {
            applied.lock().expect("applied lock").push(value);
            Ok(())
        },
        |_, _| None::<String>,
        |_| {},
    )
    .expect("degraded run must succeed");

    assert_eq!(
        *applied.lock().expect("applied lock"),
        vec![0, 2, 3],
        "ready results after the skipped sequence must still be applied in order"
    );
}

#[test]
fn extraction_gate_serializes_data_source_runs() {
    let gate = Arc::new(ExtractionGate::new());
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let first = gate
        .acquire(&cancel, |_| {})
        .expect("first extraction should acquire the gate");
    let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let gate_for_thread = Arc::clone(&gate);
    let cancel_for_thread = Arc::clone(&cancel);
    let entered_for_thread = Arc::clone(&entered);
    let waiter = std::thread::spawn(move || {
        let guard = gate_for_thread
            .acquire(&cancel_for_thread, |_| {})
            .expect("second extraction should acquire after the first finishes");
        entered_for_thread.store(true, Ordering::SeqCst);
        drop(guard);
    });

    std::thread::sleep(std::time::Duration::from_millis(40));
    assert!(!entered.load(Ordering::SeqCst));
    drop(first);
    waiter.join().expect("gate waiter should finish");
    assert!(entered.load(Ordering::SeqCst));
}
