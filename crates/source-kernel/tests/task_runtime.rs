use source_kernel::task_runtime::{process_owner, TaskManager, TaskPriority, TaskScope};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

#[test]
fn parallel_queue_serializes_each_source_and_runs_an_independent_source() {
    let manager = TaskManager::new();
    let release = Arc::new(AtomicBool::new(false));
    let first_release = Arc::clone(&release);
    let (entered, entry) = mpsc::channel();
    manager
        .spawn_scoped_parallel(
            "a-first".into(),
            TaskScope::data_source("case", "a", "a"),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                entered.send(()).unwrap();
                while !first_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .unwrap();
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    let second_started = Arc::new(AtomicBool::new(false));
    let second_flag = Arc::clone(&second_started);
    manager
        .spawn_scoped_parallel(
            "a-second".into(),
            TaskScope::data_source("case", "a", "a"),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                second_flag.store(true, Ordering::Release);
                Ok(())
            },
        )
        .unwrap();
    let (independent, observed) = mpsc::channel();
    manager
        .spawn_scoped_parallel(
            "b".into(),
            TaskScope::data_source("case", "b", "b"),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                independent.send(()).unwrap();
                Ok(())
            },
        )
        .unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!second_started.load(Ordering::Acquire));
    release.store(true, Ordering::Release);
    assert_eq!(
        manager.wait_task("a-first", Duration::from_secs(2)),
        Some(Ok(()))
    );
    assert_eq!(
        manager.wait_task("a-second", Duration::from_secs(2)),
        Some(Ok(()))
    );
    assert_eq!(manager.wait_task("b", Duration::from_secs(2)), Some(Ok(())));
}

#[test]
fn cancelled_parallel_worker_retains_capacity_until_the_worker_returns() {
    let manager = TaskManager::new();
    let (entered, observed) = mpsc::channel();
    let (release_first, wait_first) = mpsc::sync_channel(0);
    let (release_second, wait_second) = mpsc::sync_channel(0);
    for (id, source, wait) in [("first", "a", wait_first), ("second", "b", wait_second)] {
        let entered = entered.clone();
        manager
            .spawn_scoped_parallel(
                id.into(),
                TaskScope::data_source("case", source, id),
                TaskPriority::Interactive,
                Arc::new(AtomicBool::new(false)),
                move || {
                    entered.send(id).unwrap();
                    wait.recv().unwrap();
                    Ok(())
                },
            )
            .unwrap();
    }
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(manager.cancel("first"));
    let third_entered = entered.clone();
    manager
        .spawn_scoped_parallel(
            "third".into(),
            TaskScope::data_source("case", "c", "third"),
            TaskPriority::Interactive,
            Arc::new(AtomicBool::new(false)),
            move || {
                third_entered.send("third").unwrap();
                Ok(())
            },
        )
        .unwrap();
    assert!(observed.recv_timeout(Duration::from_millis(100)).is_err());
    release_first.send(()).unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(2)).unwrap(),
        "third"
    );
    release_second.send(()).unwrap();
    for (_, result) in manager.wait_all(Duration::from_secs(2)) {
        result.unwrap();
    }
    assert_eq!(manager.task_count(), 0);
}

#[test]
fn interactive_work_precedes_queued_background_work_for_the_same_source() {
    let manager = TaskManager::new();
    let release = Arc::new(AtomicBool::new(false));
    let worker_release = Arc::clone(&release);
    let (entered, entry) = mpsc::channel();
    manager
        .spawn_scoped_parallel(
            "first".into(),
            TaskScope::data_source("case", "a", "a"),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                entered.send(()).unwrap();
                while !worker_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .unwrap();
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    let (sender, receiver) = mpsc::channel();
    for (id, priority) in [
        (2, TaskPriority::Background),
        (3, TaskPriority::Interactive),
    ] {
        let sender = sender.clone();
        manager
            .spawn_scoped_parallel(
                format!("queued-{id}"),
                TaskScope::data_source("case", "a", "a"),
                priority,
                Arc::new(AtomicBool::new(false)),
                move || {
                    sender.send(id).unwrap();
                    Ok(())
                },
            )
            .unwrap();
    }
    release.store(true, Ordering::Release);
    assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 3);
    assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    assert_eq!(
        manager.wait_task("first", Duration::from_secs(2)),
        Some(Ok(()))
    );
}

#[test]
fn retirement_drains_queued_parallel_work_without_starting_it() {
    let manager = TaskManager::new();
    let (entered, entry) = mpsc::channel();
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = Arc::clone(&token);
    manager
        .spawn_scoped_parallel(
            "running".into(),
            TaskScope::data_source("case", "a", "a"),
            TaskPriority::Background,
            token,
            move || {
                entered.send(()).unwrap();
                while !worker_token.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .unwrap();
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    manager
        .spawn_scoped_parallel(
            "queued".into(),
            TaskScope::data_source("case", "a", "a"),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                worker_started.store(true, Ordering::Release);
                Ok(())
            },
        )
        .unwrap();
    manager.retire_case_and_drain("case", Duration::from_secs(2));
    assert_eq!(manager.task_count_for_case("case"), 0);
    assert!(!started.load(Ordering::Acquire));
}

#[test]
fn process_owner_child() {
    let owner = process_owner::current_owner().unwrap();
    assert!(process_owner::owner_is_running(&owner).unwrap());
    if let Some(path) = std::env::var_os("MEOW_TEST_PROCESS_OWNER_OUTPUT") {
        std::fs::write(path, owner).unwrap();
    }
}

#[test]
fn exited_process_owner_is_not_considered_live() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("owner.json");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_owner_child", "--nocapture"])
        .env("MEOW_TEST_PROCESS_OWNER_OUTPUT", &output)
        .status()
        .unwrap();
    assert!(status.success());
    let owner = std::fs::read_to_string(output).unwrap();
    assert!(!process_owner::owner_is_running(&owner).unwrap());
}
