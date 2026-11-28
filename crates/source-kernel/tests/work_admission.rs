use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

use domain::structure::ReadPriority;
use source_kernel::{
    resource_budget::{ResourcePool, Resources},
    source_runtime::work_admission::WorkAdmission,
};

#[test]
fn waiting_interactive_read_precedes_deferred_scan_using_the_existing_cpu_pool() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-work-admission");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = tempfile::tempdir_in(root).unwrap();
    let path = workspace.path().join("source.raw");
    std::fs::write(&path, b"actual source bytes").unwrap();
    let pool = ResourcePool::new(Resources {
        cpu: 1,
        ..Default::default()
    });
    let admission = WorkAdmission::new(pool.clone());
    let occupied = admission
        .acquire(ReadPriority::Normal, &AtomicBool::new(false), None)
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut tasks = Vec::new();
    for (priority, label) in [
        (ReadPriority::Deferred, "scan"),
        (ReadPriority::Interactive, "preview"),
    ] {
        let worker_admission = admission.clone();
        let sender = sender.clone();
        let path = path.clone();
        tasks.push(std::thread::spawn(move || {
            let _permit = worker_admission
                .acquire(priority, &AtomicBool::new(false), None)
                .unwrap();
            let mut source = std::fs::File::open(path).unwrap();
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"actual source bytes");
            sender.send(label).unwrap();
        }));
        wait_for(|| admission.pending().unwrap()[priority as usize] == 1);
    }
    drop(occupied);
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        "preview"
    );
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        "scan"
    );
    for task in tasks {
        task.join().unwrap();
    }
    assert_eq!(admission.pending().unwrap(), [0; 3]);
    assert_eq!(pool.snapshot().0, Resources::default());
    assert_eq!(pool.snapshot().1.cpu, 1);
}

#[test]
fn cancelled_admission_releases_its_waiter_without_consuming_cpu_capacity() {
    let pool = ResourcePool::new(Resources {
        cpu: 1,
        ..Default::default()
    });
    let admission = WorkAdmission::new(pool.clone());
    let occupied = admission
        .acquire(ReadPriority::Normal, &AtomicBool::new(false), None)
        .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel = cancelled.clone();
    let waiting = admission.clone();
    let task = std::thread::spawn(move || {
        waiting
            .acquire(ReadPriority::Deferred, &cancel, None)
            .is_err()
    });
    wait_for(|| admission.pending().unwrap()[ReadPriority::Deferred as usize] == 1);
    cancelled.store(true, Ordering::Release);
    assert!(task.join().unwrap());
    assert_eq!(admission.pending().unwrap(), [0; 3]);
    assert_eq!(pool.snapshot().0.cpu, 1);
    drop(occupied);
    assert_eq!(pool.snapshot().0.cpu, 0);
}

fn wait_for(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
