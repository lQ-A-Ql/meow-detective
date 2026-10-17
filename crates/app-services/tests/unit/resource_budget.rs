use super::*;
#[test]
fn vector_admission_never_overcommits_and_drop_releases_every_dimension() {
    let pool = ResourcePool::new(Resources {
        bytes: 100,
        metadata: 50,
        readers: 2,
        handles: 4,
        workers: 2,
        catalogs: 1,
    });
    let request = Resources {
        bytes: 60,
        metadata: 30,
        readers: 1,
        handles: 3,
        workers: 1,
        catalogs: 1,
    };
    let permit = pool.acquire(request, None).unwrap();
    assert!(matches!(
        pool.acquire(request, None),
        Err(ResourceError::Busy)
    ));
    assert_eq!(pool.snapshot().0, request);
    drop(permit);
    assert_eq!(pool.snapshot().0, Resources::default());
    assert!(pool.snapshot().1.fits(pool.limits()));
}
#[test]
fn parse_reservation_transfers_to_shared_metadata_without_waiting() {
    let pool = ResourcePool::new(Resources {
        bytes: 100,
        metadata: 50,
        ..Default::default()
    });
    let parse = pool
        .acquire(
            Resources {
                bytes: 100,
                metadata: 50,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let shared = Arc::new(
        parse
            .shrink(Resources {
                bytes: 30,
                metadata: 30,
                ..Default::default()
            })
            .unwrap(),
    );
    let reader = shared.clone();
    drop(shared);
    assert_eq!(pool.snapshot().0.bytes, 30);
    drop(reader);
    assert_eq!(pool.snapshot().0, Resources::default());
}
#[test]
fn waiting_for_resources_is_cancelled_and_does_not_leak_a_reservation() {
    let pool = ResourcePool::new(Resources {
        bytes: 1,
        ..Default::default()
    });
    let permit = pool
        .acquire(
            Resources {
                bytes: 1,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let thread_cancel = cancel.clone();
        let thread_pool = pool.clone();
        let worker = scope.spawn(move || {
            thread_pool.acquire(
                Resources {
                    bytes: 1,
                    ..Default::default()
                },
                Some(&thread_cancel),
            )
        });
        std::thread::sleep(Duration::from_millis(20));
        cancel.store(true, Ordering::Release);
        assert!(matches!(
            worker.join().unwrap(),
            Err(ResourceError::Cancelled)
        ));
    });
    drop(permit);
    assert_eq!(pool.snapshot().0, Resources::default());
}

#[test]
fn retired_workspace_keeps_only_borrowed_bytes_charged_until_last_drop() {
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        workers: 4,
        ..Default::default()
    });
    let job = root.workspace(&AtomicBool::new(false)).unwrap();
    let child = job.pool();
    let borrowed = child
        .acquire(
            Resources {
                bytes: 17 * MIB,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(root.snapshot().0.bytes, WORKSPACE_BYTES);
    assert_eq!(child.snapshot().0.bytes, 17 * MIB);
    drop(job);
    assert_eq!(root.snapshot().0.bytes, 17 * MIB);
    assert_eq!(root.snapshot().0.workers, 0);
    assert!(matches!(
        child.acquire(Resources::default(), None),
        Err(ResourceError::Busy)
    ));
    drop(borrowed);
    assert_eq!(root.snapshot().0, Resources::default());
}
#[test]
fn nested_phase_cannot_wait_on_resources_it_already_holds() {
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        workers: 4,
        ..Default::default()
    });
    let job = root.workspace(&AtomicBool::new(false)).unwrap();
    let child = job.pool();
    let phase = child
        .acquire(
            Resources {
                bytes: 800 * MIB,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert!(matches!(
        child.acquire(
            Resources {
                bytes: 300 * MIB,
                ..Default::default()
            },
            Some(&AtomicBool::new(false))
        ),
        Err(ResourceError::Busy)
    ));
    drop(phase);
    drop(job);
    assert_eq!(root.snapshot().0, Resources::default());
}

#[test]
fn two_maximum_e01_metadata_owners_leave_capacity_for_one_workspace() {
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        metadata: 516 * MIB,
        workers: 4,
        ..Default::default()
    });
    let request = Resources {
        bytes: 258 * MIB,
        metadata: 258 * MIB,
        ..Default::default()
    };
    let first = root.acquire(request, None).unwrap();
    let second = root.acquire(request, None).unwrap();
    let workspace = root.workspace(&AtomicBool::new(false)).unwrap();
    assert_eq!(root.snapshot().0.bytes, root.limits().bytes);
    drop(workspace);
    drop(first);
    drop(second);
    assert_eq!(root.snapshot().0, Resources::default());
}
