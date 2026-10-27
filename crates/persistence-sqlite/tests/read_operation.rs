use persistence_sqlite::{
    open_existing_source_read_only,
    read_operation::{ensure_active, ReadCancellation, ReadStopReason},
    DbError,
};
use rusqlite::{Connection, ErrorCode};
use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const QUERY: &str = "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n";

fn database(directory: &std::path::Path) -> std::path::PathBuf {
    let path = directory.join("source.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE proof(value INTEGER); INSERT INTO proof VALUES(7);")
        .unwrap();
    path
}

#[test]
fn source_sql_is_interrupted_without_changing_database_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let path = database(temporary.path());
    let before = std::fs::read(&path).unwrap();
    let control = Arc::new(ReadCancellation::default());
    let worker_control = control.clone();
    let worker_path = path.clone();
    let (ready, started) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _scope = worker_control.enter();
        let connection = open_existing_source_read_only(&worker_path).unwrap();
        ready.send(()).unwrap();
        connection.query_row(QUERY, [], |row| row.get::<_, i64>(0))
    });
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    control.cancel();
    assert!(
        matches!(worker.join().unwrap(), Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == ErrorCode::OperationInterrupted)
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn nested_scopes_restore_state_and_reused_connections_do_not_keep_old_cancellation() {
    let temporary = tempfile::tempdir().unwrap();
    let path = database(temporary.path());
    let outer = Arc::new(ReadCancellation::default());
    let outer_scope = outer.enter();
    let connection = open_existing_source_read_only(&path).unwrap();
    outer.time_out();
    outer.cancel();
    assert_eq!(outer.reason(), Some(ReadStopReason::TimedOut));
    assert!(matches!(ensure_active(), Err(DbError::ReadTimedOut)));
    {
        let inner = Arc::new(ReadCancellation::default());
        let _scope = inner.enter();
        assert!(ensure_active().is_ok());
        let value: i64 = connection
            .query_row("SELECT value FROM proof", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, 7);
    }
    assert!(matches!(ensure_active(), Err(DbError::ReadTimedOut)));
    drop(outer_scope);
    assert!(ensure_active().is_ok());
    let sum: i64 = connection.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2000) SELECT sum(x) FROM n", [], |row| row.get(0)).unwrap();
    assert_eq!(sum, 2_001_000);
}

#[test]
fn cancellation_ends_source_lock_waiting() {
    let temporary = tempfile::tempdir().unwrap();
    let path = database(temporary.path());
    let writer = Connection::open(&path).unwrap();
    let control = Arc::new(ReadCancellation::default());
    let worker_control = control.clone();
    let (ready, started) = mpsc::channel();
    let (run, next) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _scope = worker_control.enter();
        let connection = open_existing_source_read_only(&path).unwrap();
        ready.send(()).unwrap();
        next.recv().unwrap();
        connection.query_row("SELECT value FROM proof", [], |row| row.get::<_, i64>(0))
    });
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    writer.execute_batch("BEGIN EXCLUSIVE").unwrap();
    control.cancel();
    let start = Instant::now();
    run.send(()).unwrap();
    assert!(
        matches!(worker.join().unwrap(), Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == ErrorCode::DatabaseBusy)
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    writer.execute_batch("ROLLBACK").unwrap();
}
