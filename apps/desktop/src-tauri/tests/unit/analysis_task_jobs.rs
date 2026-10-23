use std::{path::PathBuf, sync::Arc};

use persistence_sqlite::repositories::job_repo::JobRepo;
use tempfile::TempDir;
use transport::CommandError;

use super::execute;

struct Fixture {
    _temp: TempDir,
    db_path: PathBuf,
    job_id: domain::JobId,
}

fn fixture(name: &str) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let active = app_services::case_service::create_case(temp.path(), name, None).unwrap();
    let db_path = active.db_path();
    let connection = app_services::connection::open_existing_case_db(&db_path).unwrap();
    let job_id = JobRepo::new(&connection)
        .create(&active.meta.id.0, "Analysis test")
        .unwrap();
    drop(connection);
    Fixture {
        _temp: temp,
        db_path,
        job_id,
    }
}

fn status(fixture: &Fixture) -> String {
    let connection = app_services::connection::open_existing_case_db(&fixture.db_path).unwrap();
    JobRepo::new(&connection).list_recent(1).unwrap()[0]
        .status
        .clone()
}

#[test]
fn committed_operation_marks_job_completed() {
    let fixture = fixture("analysis-managed-success");
    let value = execute::<_, _>(
        None,
        &fixture.db_path,
        &fixture.job_id.0,
        "Analysis test",
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        |_, _| Ok(7),
    )
    .unwrap();

    assert_eq!(value, 7);
    assert_eq!(status(&fixture), "completed");
}

#[test]
fn cancelled_operation_keeps_cancelled_terminal_state() {
    let fixture = fixture("analysis-managed-cancelled");
    let error = execute::<(), _>(
        None,
        &fixture.db_path,
        &fixture.job_id.0,
        "Analysis test",
        Arc::new(std::sync::atomic::AtomicBool::new(true)),
        |_, _| Err(CommandError::cancelled("cancelled by test")),
    )
    .unwrap_err();

    assert_eq!(error.category, "cancelled");
    assert_eq!(status(&fixture), "cancelled");
}

#[test]
fn failed_operation_marks_job_failed() {
    let fixture = fixture("analysis-managed-failed");
    let error = execute::<(), _>(
        None,
        &fixture.db_path,
        &fixture.job_id.0,
        "Analysis test",
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        |_, _| Err(CommandError::internal("analysis failed")),
    )
    .unwrap_err();

    assert_eq!(error.category, "internal");
    assert_eq!(status(&fixture), "failed");
}
