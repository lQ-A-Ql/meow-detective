use super::*;
use crate::commands::command_support::{require_active_case, run_case_read_command};
use std::time::Duration;

fn case() -> (tempfile::TempDir, AppState) {
    let temp = tempfile::tempdir().unwrap();
    let active = app_services::case_service::create_case(temp.path(), "access", None).unwrap();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(active);
    (temp, state)
}

#[tokio::test]
async fn connections_and_snapshots_prevent_case_transition_until_released() {
    let (_temp, state) = case();
    let snapshot = require_active_case(&state).unwrap();
    let first = state.get_read_connection().unwrap();
    let second = state.get_read_connection().unwrap();
    assert!(state.case_lifecycle.try_write().is_err());
    drop(snapshot);
    drop(first);
    assert!(state.case_lifecycle.try_write().is_err());
    drop(second);
    let transition = state.case_transition().await.unwrap();
    assert_eq!(
        require_active_case(&state).unwrap_err().category,
        "validation"
    );
    assert!(state.get_connection().is_err());
    drop(transition);
    assert!(state.get_connection().is_ok());
}

#[test]
fn writer_policy_is_explicit_and_reader_rejects_mutation() {
    let (_temp, state) = case();
    let writer = state.get_connection().unwrap();
    for (pragma, expected) in [
        ("foreign_keys", 1),
        ("synchronous", 1),
        ("busy_timeout", 30_000),
    ] {
        let actual: i64 = writer
            .pragma_query_value(None, pragma, |row| row.get(0))
            .unwrap();
        assert_eq!(actual, expected, "{pragma}");
    }
    let reader = state.get_read_connection().unwrap();
    assert_eq!(
        reader
            .pragma_query_value(None, "query_only", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        reader
            .execute("UPDATE cases SET name='forbidden'", [])
            .unwrap_err()
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::ReadOnly)
    );
}

#[tokio::test]
async fn aborted_async_query_retains_access_until_blocking_worker_exits() {
    let (_temp, state) = case();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker_state = state.clone();
    let task = tokio::spawn(async move {
        run_case_read_command(worker_state, move |conn, _| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            conn.query_row("SELECT count(*) FROM cases", [], |row| row.get::<_, i64>(0))
                .map_err(CommandError::from_service_error)
        })
        .await
    });
    started_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(state.case_lifecycle.try_write().is_err());
    release_tx.send(()).unwrap();
    let access = tokio::time::timeout(Duration::from_secs(2), state.case_lifecycle.write())
        .await
        .unwrap();
    drop(access);
}

#[test]
fn exhausted_pool_does_not_hold_pool_registry_mutex() {
    let (_temp, state) = case();
    let readers = (0..4)
        .map(|_| state.get_read_connection().unwrap())
        .collect::<Vec<_>>();
    let worker_state = state.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        worker_state.get_read_connection().unwrap()
    });
    started_rx.recv().unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert!(state.read_pool.try_lock().is_ok());
    assert!(state.case_lifecycle.try_write().is_err());
    drop(readers);
    drop(worker.join().unwrap());
    assert!(state.case_lifecycle.try_write().is_ok());
}

#[tokio::test]
async fn transition_timeout_preserves_case_and_allows_retry() {
    let (_temp, state) = case();
    let reader = state.get_read_connection().unwrap();
    let error = state.case_transition().await.unwrap_err();
    assert_eq!(error.category, "timeout");
    assert!(state.active_case.lock().unwrap().is_some());
    assert!(state.read_pool.lock().unwrap().is_some());
    assert!(state.clear_db_state().is_err());
    drop(reader);
    assert!(state.case_transition().await.is_ok());
}

#[test]
fn missing_case_database_is_not_created_by_writer_checkout() {
    let (temp, state) = case();
    let missing = temp.path().join("missing-workspace");
    state
        .active_case
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .case_root = missing.clone();
    assert!(state.get_connection().is_err());
    assert!(!missing.exists());
}
