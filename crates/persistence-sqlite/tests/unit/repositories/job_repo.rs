use super::*;

fn setup_db() -> rusqlite::Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE jobs (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            progress INTEGER NOT NULL DEFAULT 0,
            detail TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            started_at TEXT,
            finished_at TEXT,
            current_partition TEXT DEFAULT NULL,
            completed_partitions INTEGER DEFAULT 0,
            total_partitions INTEGER DEFAULT 0,
            partition_progress INTEGER DEFAULT 0,
            warning_count INTEGER NOT NULL DEFAULT 0,
            skipped_count INTEGER NOT NULL DEFAULT 0,
            failed_count INTEGER NOT NULL DEFAULT 0,
            partial INTEGER NOT NULL DEFAULT 0
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
        params!["case-1", "Test Case"],
    ).unwrap();
    conn
}

#[test]
fn create_returns_job_id() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);
    let id = repo.create("case-1", "ingest").unwrap();
    assert!(!id.0.is_empty());
}

#[test]
fn update_progress_changes_progress() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);
    let id = repo.create("case-1", "ingest").unwrap();

    repo.update_progress(&id, 50, "halfway").unwrap();

    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].progress, 50);
    assert_eq!(jobs[0].detail, "halfway");
}

#[test]
fn complete_sets_status_to_completed() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);
    let id = repo.create("case-1", "ingest").unwrap();

    repo.complete(&id, "done").unwrap();

    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs[0].status, "completed");
    assert_eq!(jobs[0].progress, 100);
}

#[test]
fn fail_sets_status_to_failed() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);
    let id = repo.create("case-1", "ingest").unwrap();

    repo.fail(&id, "error occurred").unwrap();

    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs[0].status, "failed");
    assert_eq!(jobs[0].detail, "error occurred");
}

#[test]
fn cancellation_methods_update_status_without_schema_changes() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);
    let id = repo.create("case-1", "ingest").unwrap();

    repo.mark_cancelling(&id, "Cancel requested").unwrap();
    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs[0].status, "cancelling");
    assert_eq!(jobs[0].detail, "Cancel requested");

    repo.cancel(&id, "Import cancelled by user").unwrap();
    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs[0].status, "cancelled");
    assert_eq!(jobs[0].detail, "Import cancelled by user");
}

#[test]
fn list_recent_returns_jobs_ordered() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);

    let id1 = repo.create("case-1", "ingest").unwrap();
    let _id2 = repo.create("case-1", "search").unwrap();
    repo.complete(&id1, "done").unwrap();

    let jobs = repo.list_recent(10).unwrap();
    assert_eq!(jobs.len(), 2);
    // Running/pending jobs come before completed ones
    assert_eq!(jobs[0].status, "running");
    assert_eq!(jobs[1].status, "completed");
}

#[test]
fn find_interrupted_returns_only_running_and_cancelling() {
    let conn = setup_db();
    let repo = JobRepo::new(&conn);

    let running = repo.create("case-1", "import").unwrap();
    let cancelling = repo.create("case-1", "import").unwrap();
    repo.mark_cancelling(&cancelling, "test").unwrap();

    let completed = repo.create("case-1", "import").unwrap();
    repo.complete(&completed, "done").unwrap();

    let failed = repo.create("case-1", "import").unwrap();
    repo.fail(&failed, "err").unwrap();

    let interrupted = repo.find_interrupted().unwrap();
    let ids: Vec<&str> = interrupted.iter().map(|id| id.0.as_str()).collect();
    assert_eq!(interrupted.len(), 2);
    assert!(ids.contains(&running.0.as_str()));
    assert!(ids.contains(&cancelling.0.as_str()));
}
