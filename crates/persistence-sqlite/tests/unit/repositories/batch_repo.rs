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
        CREATE TABLE batch_jobs (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
            label TEXT NOT NULL DEFAULT '',
            plan_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'queued',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            started_at TEXT,
            completed_at TEXT
        );
        CREATE TABLE batch_phases (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'queued',
            progress REAL NOT NULL DEFAULT 0.0,
            started_at TEXT,
            completed_at TEXT,
            error_count INTEGER NOT NULL DEFAULT 0,
            warnings_json TEXT NOT NULL DEFAULT '[]',
            UNIQUE(batch_id, kind),
            FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
        );
        CREATE TABLE batch_checkpoints (
            batch_id TEXT NOT NULL,
            phase_kind TEXT NOT NULL,
            key TEXT NOT NULL,
            value_json TEXT NOT NULL DEFAULT '{}',
            saved_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (batch_id, phase_kind, key),
            FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
        params!["case-1", "Test Case"],
    )
    .unwrap();
    conn
}

#[test]
fn create_and_get_batch_job() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job(
        "batch-1",
        "case-1",
        "Test Batch",
        r#"{"phases":["Mount","Catalog"]}"#,
    )
    .unwrap();

    let job = repo.get_job("batch-1").unwrap().expect("should exist");
    assert_eq!(job.id, "batch-1");
    assert_eq!(job.case_id, "case-1");
    assert_eq!(job.label, "Test Batch");
    assert_eq!(job.status, "queued");
}

#[test]
fn list_jobs_by_case() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job("batch-1", "case-1", "First", "{}").unwrap();
    repo.create_job("batch-2", "case-1", "Second", "{}")
        .unwrap();

    let jobs = repo.list_jobs("case-1").unwrap();
    assert_eq!(jobs.len(), 2);
}

#[test]
fn count_jobs_by_status_uses_one_case_scope() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-2', 'Other', datetime('now'), datetime('now'))",
        [],
    )
    .unwrap();

    for (id, status) in [
        ("running", "running"),
        ("starting", "starting"),
        ("completed", "completed"),
        ("failed", "failed"),
        ("queued", "queued"),
        ("cancelled", "cancelled"),
    ] {
        repo.create_job(id, "case-1", id, "{}").unwrap();
        repo.update_job_status(id, status).unwrap();
    }
    repo.create_job("other", "case-2", "Other", "{}").unwrap();

    let counts = repo.count_jobs_by_status("case-1").unwrap();
    assert_eq!(counts.active_jobs, 2);
    assert_eq!(counts.completed_jobs, 1);
    assert_eq!(counts.failed_jobs, 1);
    assert_eq!(counts.queued_jobs, 1);
    assert_eq!(counts.total_jobs, 6);
    assert_eq!(
        repo.count_jobs_by_status("missing").unwrap(),
        Default::default()
    );
}

#[test]
fn update_job_status() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
    repo.update_job_status("batch-1", "running").unwrap();

    let job = repo.get_job("batch-1").unwrap().unwrap();
    assert_eq!(job.status, "running");
    assert!(job.started_at.is_some());
}

#[test]
fn upsert_and_get_phases() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
    repo.upsert_phase("batch-1", "Mount", "running", 0.5, 0, "[]")
        .unwrap();
    repo.upsert_phase("batch-1", "Mount", "completed", 1.0, 0, "[]")
        .unwrap();

    let phases = repo.get_phases("batch-1").unwrap();
    assert_eq!(phases.len(), 1);
    assert_eq!(phases[0].kind, "Mount");
    assert_eq!(phases[0].state, "completed");
    assert_eq!(phases[0].progress, 1.0);
}

#[test]
fn checkpoint_read_write() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
    repo.write_checkpoint("batch-1", "Catalog", "last_offset", r#""12345""#)
        .unwrap();

    let val = repo
        .read_checkpoint("batch-1", "Catalog", "last_offset")
        .unwrap();
    assert_eq!(val.unwrap(), r#""12345""#);
}

#[test]
fn checkpoint_missing_returns_none() {
    let conn = setup_db();
    let repo = BatchRepo::new(&conn);

    repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
    let val = repo
        .read_checkpoint("batch-1", "Catalog", "nonexistent")
        .unwrap();
    assert!(val.is_none());
}
