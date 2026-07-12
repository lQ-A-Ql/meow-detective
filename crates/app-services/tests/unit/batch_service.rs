use super::*;
use rusqlite::params;

fn setup_db() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    // Run the batch migration tables manually
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
fn create_plan_parses_phases() {
    let dto = BatchPlanDto {
        data_source_refs: vec!["ds1".into()],
        phases: vec!["Mount".into(), "Catalog".into()],
        resource_limits: BatchResourceLimitsDto {
            max_memory_mb: Some(1024),
            max_threads: Some(4),
        },
    };
    let plan = create_batch_plan(dto).unwrap();
    assert_eq!(plan.phases.len(), 2);
    assert_eq!(plan.phases[0], PhaseKind::Mount);
    assert_eq!(plan.phases[1], PhaseKind::Catalog);
    assert_eq!(plan.resource_limits.max_memory_mb, Some(1024));
}

#[test]
fn create_plan_rejects_unknown_phase() {
    let dto = BatchPlanDto {
        data_source_refs: vec![],
        phases: vec!["UnknownKind".into()],
        resource_limits: BatchResourceLimitsDto {
            max_memory_mb: None,
            max_threads: None,
        },
    };
    assert!(create_batch_plan(dto).is_err());
}

#[test]
fn create_and_get_batch_status() {
    let conn = setup_db();

    let plan_dto = BatchPlanDto {
        data_source_refs: vec!["ds1".into()],
        phases: vec!["Mount".into(), "Index".into()],
        resource_limits: BatchResourceLimitsDto {
            max_memory_mb: Some(512),
            max_threads: None,
        },
    };
    let job = create_and_persist_batch(&conn, "case-1", "my batch", plan_dto).unwrap();

    assert!(!job.id.is_empty());
    assert_eq!(job.label, "my batch");
    assert_eq!(job.status, "queued");
    assert_eq!(job.phases.len(), 2);
    assert_eq!(job.phases[0].kind, "Mount");
    assert_eq!(job.phases[0].state, "queued");
    assert_eq!(job.phases[1].kind, "Index");
}

#[test]
fn list_batch_jobs_returns_for_case() {
    let conn = setup_db();

    let plan = BatchPlanDto {
        data_source_refs: vec!["ds1".into()],
        phases: vec!["Export".into()],
        resource_limits: BatchResourceLimitsDto {
            max_memory_mb: None,
            max_threads: None,
        },
    };
    create_and_persist_batch(&conn, "case-1", "batch A", plan).unwrap();

    let jobs = list_batch_jobs(&conn, "case-1").unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].label, "batch A");
}

#[test]
fn get_batch_status_for_unknown_id_returns_error() {
    let conn = setup_db();
    assert!(get_batch_status(&conn, "nonexistent").is_err());
}

#[test]
fn start_pause_resume_cancel_are_stubs() {
    let conn = setup_db();
    assert!(start_batch(&conn, "batch-1").is_err());
    assert!(pause_batch(&conn, "batch-1").is_err());
    let resume = BatchResumeDto {
        batch_id: "batch-1".into(),
        resource_limits: None,
    };
    assert!(resume_batch(&conn, resume).is_err());
    assert!(cancel_batch(&conn, "batch-1").is_err());
}
