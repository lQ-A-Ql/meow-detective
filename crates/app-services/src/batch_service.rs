use domain::batch::{BatchJob, BatchPhase, BatchPlan, BatchResourceLimits, PhaseKind, PhaseState};
use persistence_sqlite::repositories::batch_repo::BatchRepo;
use rusqlite::Connection;
use transport::dto::batch::{
    BatchJobDto, BatchPhaseDto, BatchPlanDto, BatchResourceLimitsDto, BatchResumeDto,
};
use uuid::Uuid;

/// Build a `BatchPlan` from a request DTO.
pub fn create_batch_plan(dto: BatchPlanDto) -> Result<BatchPlan, String> {
    let phases: Vec<PhaseKind> = dto
        .phases
        .iter()
        .map(|p| parse_phase_kind(p))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BatchPlan {
        data_source_refs: dto.data_source_refs,
        phases,
        resource_limits: BatchResourceLimits {
            max_memory_mb: dto.resource_limits.max_memory_mb,
            max_threads: dto.resource_limits.max_threads,
        },
    })
}

/// Create a new batch job in the database. Returns the job DTO.
/// MVP: status is set to `queued`; the caller must call `start_batch` separately.
pub fn create_and_persist_batch(
    conn: &Connection,
    case_id: &str,
    label: &str,
    plan_dto: BatchPlanDto,
) -> Result<BatchJobDto, String> {
    let plan = create_batch_plan(plan_dto)?;
    let plan_json =
        serde_json::to_string(&plan).map_err(|e| format!("Failed to serialize plan: {e}"))?;

    let batch_id = Uuid::new_v4().to_string();

    let repo = BatchRepo::new(conn);
    repo.create_job(&batch_id, case_id, label, &plan_json)
        .map_err(|e| e.to_string())?;

    for kind in &plan.phases {
        repo.upsert_phase(
            &batch_id,
            &phase_kind_to_str(kind),
            "queued",
            0.0,
            0,
            "[]",
        )
        .map_err(|e| e.to_string())?;
    }

    get_batch_status(conn, &batch_id)
}

/// Retrieve the full status of a batch job, including all phases.
pub fn get_batch_status(conn: &Connection, batch_id: &str) -> Result<BatchJobDto, String> {
    let repo = BatchRepo::new(conn);

    let job_row = repo
        .get_job(batch_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Batch job not found: {batch_id}"))?;

    let plan: BatchPlan = serde_json::from_str(&job_row.plan_json)
        .map_err(|e| format!("Failed to deserialize plan: {e}"))?;

    let phase_rows = repo.get_phases(batch_id).map_err(|e| e.to_string())?;

    let phases: Vec<BatchPhaseDto> = phase_rows
        .into_iter()
        .map(|pr| {
            let warnings: Vec<String> =
                serde_json::from_str(&pr.warnings_json).unwrap_or_default();
            BatchPhaseDto {
                kind: pr.kind,
                state: pr.state,
                progress: pr.progress,
                started_at: pr.started_at,
                completed_at: pr.completed_at,
                error_count: pr.error_count,
                warnings,
            }
        })
        .collect();

    Ok(BatchJobDto {
        id: job_row.id,
        case_id: job_row.case_id,
        label: job_row.label,
        plan: BatchPlanDto {
            data_source_refs: plan.data_source_refs,
            phases: plan.phases.iter().map(phase_kind_to_str).collect(),
            resource_limits: BatchResourceLimitsDto {
                max_memory_mb: plan.resource_limits.max_memory_mb,
                max_threads: plan.resource_limits.max_threads,
            },
        },
        phases,
        created_at: job_row.created_at,
        started_at: job_row.started_at,
        completed_at: job_row.completed_at,
        status: job_row.status,
    })
}

/// List all batch jobs for a case.
pub fn list_batch_jobs(conn: &Connection, case_id: &str) -> Result<Vec<BatchJobDto>, String> {
    let repo = BatchRepo::new(conn);
    let rows = repo.list_jobs(case_id).map_err(|e| e.to_string())?;
    rows.into_iter()
        .map(|row| get_batch_status(conn, &row.id))
        .collect()
}

// --- Stubs (MVP: create_plan + get_status are real; start/pause/resume are stubs) ---

/// Start a queued batch job.  MVP stub.
pub fn start_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, String> {
    // TODO: spawn async task that iterates phases, setting status to running/completed.
    Err("batch start is not yet implemented (MVP stub)".to_string())
}

/// Pause a running batch job.  MVP stub.
pub fn pause_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, String> {
    // TODO: signal the running task to pause at the next checkpoint boundary.
    Err("batch pause is not yet implemented (MVP stub)".to_string())
}

/// Resume a paused batch job.  MVP stub.
pub fn resume_batch(
    _conn: &Connection,
    _resume: BatchResumeDto,
) -> Result<BatchJobDto, String> {
    // TODO: restart the task from the last checkpoint of the paused phase.
    Err("batch resume is not yet implemented (MVP stub)".to_string())
}

/// Cancel a batch job (queued or running).  MVP stub.
pub fn cancel_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, String> {
    // TODO: signal cancellation and roll back to last checkpoint.
    Err("batch cancel is not yet implemented (MVP stub)".to_string())
}

// --- helpers ---

fn parse_phase_kind(s: &str) -> Result<PhaseKind, String> {
    match s {
        "Mount" => Ok(PhaseKind::Mount),
        "Catalog" => Ok(PhaseKind::Catalog),
        "ExtractArtifacts" => Ok(PhaseKind::ExtractArtifacts),
        "Index" => Ok(PhaseKind::Index),
        "Correlate" => Ok(PhaseKind::Correlate),
        "Export" => Ok(PhaseKind::Export),
        other => Err(format!("Unknown phase kind: {other}")),
    }
}

fn phase_kind_to_str(k: &PhaseKind) -> String {
    match k {
        PhaseKind::Mount => "Mount".to_string(),
        PhaseKind::Catalog => "Catalog".to_string(),
        PhaseKind::ExtractArtifacts => "ExtractArtifacts".to_string(),
        PhaseKind::Index => "Index".to_string(),
        PhaseKind::Correlate => "Correlate".to_string(),
        PhaseKind::Export => "Export".to_string(),
    }
}

#[cfg(test)]
mod tests {
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
}
