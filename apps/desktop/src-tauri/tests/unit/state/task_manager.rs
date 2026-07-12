use super::*;
use std::thread;

#[test]
fn register_and_cancel() {
    let manager = TaskManager::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    let handle = thread::spawn(move || {
        for _ in 0..100 {
            if cancel_clone.load(Ordering::Relaxed) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err("Not cancelled".to_string())
    });

    manager.register_with_token("test-task".to_string(), handle, cancel.clone());
    assert!(manager.is_running("test-task"));
    assert_eq!(manager.task_count(), 1);

    manager.cancel("test-task");
    assert!(manager.is_cancelled("test-task"));

    let result = manager.wait_task("test-task", Duration::from_secs(5));
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
}

#[test]
fn cancel_all_tasks() {
    let manager = TaskManager::new();

    for i in 0..3 {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        let handle = thread::spawn(move || {
            for _ in 0..100 {
                if cancel_clone.load(Ordering::Relaxed) {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(format!("Task {} not cancelled", i))
        });

        manager.register_with_token(format!("task-{}", i), handle, cancel);
    }

    assert_eq!(manager.task_count(), 3);

    manager.cancel_all();
    let results = manager.wait_all(Duration::from_secs(5));
    assert_eq!(results.len(), 3);

    for (_, result) in results {
        assert!(result.is_ok());
    }
}

#[test]
fn cleanup_finished_tasks() {
    let manager = TaskManager::new();

    let handle = thread::spawn(|| Ok(()));
    manager.register("short-task".to_string(), handle);

    thread::sleep(Duration::from_millis(100));

    let cleaned = manager.cleanup_finished();
    assert_eq!(cleaned, 1);
    assert_eq!(manager.task_count(), 0);
}

#[test]
fn task_elapsed() {
    let manager = TaskManager::new();

    let handle = thread::spawn(|| {
        thread::sleep(Duration::from_millis(100));
        Ok(())
    });
    manager.register("timed-task".to_string(), handle);

    thread::sleep(Duration::from_millis(50));
    let elapsed = manager.task_elapsed("timed-task");
    assert!(elapsed.is_some());
    assert!(elapsed.unwrap() >= Duration::from_millis(50));

    manager.wait_task("timed-task", Duration::from_secs(1));
}
