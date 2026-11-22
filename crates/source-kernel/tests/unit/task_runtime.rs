use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::thread;

#[test]
fn spawn_and_cancel() {
    let manager = TaskManager::new();
    let allow_completion = Arc::new(AtomicBool::new(false));
    let task_allow_completion = Arc::clone(&allow_completion);
    let cancel = manager
        .spawn("test-task".to_string(), move |cancel| {
            while !cancel.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
            }
            while !task_allow_completion.load(Ordering::Acquire) {
                thread::yield_now();
            }
            Ok(())
        })
        .expect("spawn task");

    assert!(manager.is_running("test-task"));
    assert_eq!(manager.task_count(), 1);
    assert!(!cancel.load(Ordering::Acquire));
    assert!(manager.cancel("test-task"));
    assert!(manager.is_cancelled("test-task"));
    allow_completion.store(true, Ordering::Release);
    assert_eq!(
        manager.wait_task("test-task", Duration::from_secs(1)),
        Some(Ok(()))
    );
}

#[test]
fn completed_tasks_leave_the_active_registry_without_manual_cleanup() {
    let manager = TaskManager::new();
    manager
        .spawn("short-task".to_string(), |_| Ok(()))
        .expect("spawn task");

    for _ in 0..100 {
        if !manager.is_running("short-task") {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    assert!(!manager.is_running("short-task"));
    assert_eq!(manager.task_count(), 0);
    assert_eq!(
        manager.wait_task("short-task", Duration::from_secs(1)),
        Some(Ok(()))
    );
}

#[test]
fn duplicate_task_id_is_rejected_without_replacing_the_first_task() {
    let manager = TaskManager::new();
    manager
        .spawn("duplicate".to_string(), |cancel| {
            while !cancel.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        })
        .expect("spawn first task");

    let error = manager
        .spawn("duplicate".to_string(), |_| Ok(()))
        .expect_err("duplicate task id must be rejected");
    assert!(matches!(
        error,
        TaskRegistrationError::DuplicateTaskId(task_id) if task_id == "duplicate"
    ));
    assert_eq!(manager.task_count(), 1);
    manager.cancel("duplicate");
    manager.wait_task("duplicate", Duration::from_secs(1));
}

#[test]
fn scoped_case_retirement_does_not_cancel_other_cases() {
    let manager = TaskManager::new();
    spawn_waiting_scoped(
        &manager,
        "case-a-task",
        TaskScope::case("case-a", "group-a"),
        false,
    );
    spawn_waiting_scoped(
        &manager,
        "case-b-task",
        TaskScope::case("case-b", "group-b"),
        false,
    );

    let drained = manager.retire_case_and_drain("case-a", Duration::from_secs(1));

    assert_eq!(drained.len(), 1);
    assert_eq!(manager.task_count_for_case("case-a"), 0);
    assert_eq!(manager.task_count_for_case("case-b"), 1);
    manager.retire_case_and_drain("case-b", Duration::from_secs(1));
}

#[test]
fn retirement_rejects_a_late_child_before_its_body_starts() {
    let manager = TaskManager::new();
    manager.retire_case_and_drain("case-a", Duration::from_secs(1));
    let executed = Arc::new(AtomicBool::new(false));
    let worker_executed = Arc::clone(&executed);
    let error = manager
        .spawn_scoped(
            "late-child".to_string(),
            TaskScope::data_source("case-a", "source-a", "group-a"),
            Arc::new(AtomicBool::new(false)),
            move || {
                worker_executed.store(true, Ordering::Release);
                Ok(())
            },
        )
        .expect_err("retired case must reject late children");

    assert!(matches!(
        error,
        TaskRegistrationError::RetiredCase(case_id) if case_id == "case-a"
    ));
    thread::sleep(Duration::from_millis(20));
    assert!(!executed.load(Ordering::Acquire));
}

#[test]
fn explicit_case_reactivation_allows_new_tasks() {
    let manager = TaskManager::new();
    manager.retire_case_and_drain("case-a", Duration::from_secs(1));
    manager.reactivate_case("case-a");
    manager
        .spawn_scoped(
            "reactivated".to_string(),
            TaskScope::case("case-a", "group-a"),
            Arc::new(AtomicBool::new(false)),
            || Ok(()),
        )
        .expect("reactivated case should accept tasks");
    assert_eq!(
        manager.wait_task("reactivated", Duration::from_secs(1)),
        Some(Ok(()))
    );
}

#[test]
fn source_retirement_isolated_to_one_data_source() {
    let manager = TaskManager::new();
    manager.retire_source_and_drain("case-a", "source-a", Duration::from_secs(1));
    let rejected = manager.spawn_scoped(
        "source-a-task".to_string(),
        TaskScope::data_source("case-a", "source-a", "group-a"),
        Arc::new(AtomicBool::new(false)),
        || Ok(()),
    );
    assert!(matches!(
        rejected,
        Err(TaskRegistrationError::RetiredDataSource { data_source_id, .. })
            if data_source_id == "source-a"
    ));
    manager
        .spawn_scoped(
            "source-b-task".to_string(),
            TaskScope::data_source("case-a", "source-b", "group-b"),
            Arc::new(AtomicBool::new(false)),
            || Ok(()),
        )
        .expect("unrelated source should remain active");
    assert_eq!(
        manager.wait_task("source-b-task", Duration::from_secs(1)),
        Some(Ok(()))
    );
}

#[test]
fn heavy_tasks_run_one_at_a_time() {
    let manager = TaskManager::new();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    for ordinal in 0..3 {
        let worker_active = Arc::clone(&active);
        let worker_peak = Arc::clone(&peak);
        manager
            .spawn_scoped_heavy(
                format!("heavy-{ordinal}"),
                TaskScope::case("case-a", "heavy"),
                Arc::new(AtomicBool::new(false)),
                move || {
                    let now = worker_active.fetch_add(1, AtomicOrdering::AcqRel) + 1;
                    worker_peak.fetch_max(now, AtomicOrdering::AcqRel);
                    thread::sleep(Duration::from_millis(30));
                    worker_active.fetch_sub(1, AtomicOrdering::AcqRel);
                    Ok(())
                },
            )
            .expect("spawn heavy task");
    }

    let results = manager.wait_all(Duration::from_secs(2));
    assert_eq!(results.len(), 3);
    assert_eq!(peak.load(AtomicOrdering::Acquire), 1);
}

#[test]
fn task_elapsed_reports_active_runtime() {
    let manager = TaskManager::new();
    spawn_waiting_scoped(
        &manager,
        "timed-task",
        TaskScope::case("case-a", "timed"),
        false,
    );
    thread::sleep(Duration::from_millis(25));
    assert!(manager.task_elapsed("timed-task") >= Some(Duration::from_millis(20)));
    manager.retire_case_and_drain("case-a", Duration::from_secs(1));
}

fn spawn_waiting_scoped(manager: &TaskManager, task_id: &str, scope: TaskScope, heavy: bool) {
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let task = move || {
        while !worker_cancel.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    };
    if heavy {
        manager
            .spawn_scoped_heavy(task_id.to_string(), scope, cancel, task)
            .expect("spawn scoped heavy task");
    } else {
        manager
            .spawn_scoped(task_id.to_string(), scope, cancel, task)
            .expect("spawn scoped task");
    }
}
