use super::*;
use std::{sync::mpsc, time::Duration};

#[test]
fn four_transaction_planners_run_concurrently() {
    let directory = tempfile::tempdir().unwrap();
    let database = SourceDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let (entered, ready) = mpsc::channel();
    std::thread::scope(|scope| {
        let mut starts = Vec::new();
        let mut workers = Vec::new();
        for worker in 0..4 {
            let (start, release) = mpsc::channel();
            starts.push(start);
            let database = &database;
            let entered = entered.clone();
            workers.push(scope.spawn(move || {
                database.write_transaction(PersistMode::Buffer, |view| {
                    assert!(view.get(KeyspaceName::Meta, &[worker]).unwrap().is_none());
                    entered.send(()).unwrap();
                    release.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(vec![WriteAction::Put {
                        keyspace: KeyspaceName::Meta,
                        key: vec![worker],
                        value: vec![worker],
                    }])
                })
            }));
        }
        for _ in 0..4 {
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        for start in starts {
            start.send(()).unwrap();
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
    });
    let snapshot = database.read_handle().read_snapshot();
    for worker in 0..4 {
        assert_eq!(
            snapshot.get(KeyspaceName::Meta, &[worker]).unwrap(),
            Some(vec![worker])
        );
    }
}

#[test]
fn concurrent_read_modify_write_returns_a_conflict_without_losing_updates() {
    let directory = tempfile::tempdir().unwrap();
    let database = SourceDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    database
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: b"count".to_vec(),
                value: vec![0],
            }],
            PersistMode::Buffer,
        )
        .unwrap();
    let (entered, ready) = mpsc::channel();
    let results = std::thread::scope(|scope| {
        let mut starts = Vec::new();
        let mut workers = Vec::new();
        for _ in 0..2 {
            let (start, release) = mpsc::channel();
            starts.push(start);
            let database = &database;
            let entered = entered.clone();
            workers.push(scope.spawn(move || {
                database.write_transaction(PersistMode::Buffer, |view| {
                    let previous = view.get(KeyspaceName::Meta, b"count")?.unwrap()[0];
                    entered.send(()).unwrap();
                    release.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(vec![WriteAction::Put {
                        keyspace: KeyspaceName::Meta,
                        key: b"count".to_vec(),
                        value: vec![previous + 1],
                    }])
                })
            }));
        }
        for _ in 0..2 {
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        for start in starts {
            start.send(()).unwrap();
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(FjallStorageError::WriteConflict)))
            .count(),
        1
    );
    assert_eq!(
        database
            .read_snapshot()
            .get(KeyspaceName::Meta, b"count")
            .unwrap(),
        Some(vec![1])
    );
}
