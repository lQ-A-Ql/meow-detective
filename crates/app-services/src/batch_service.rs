//! Batch orchestration service.
//!
//! MVP scope: `create_batch_plan`, `get_batch_status`, and `list_batch_jobs` are
//! fully implemented. Execution control commands (`start_batch`, `pause_batch`,
//! `resume_batch`, `cancel_batch`) are stubs and return
//! `BatchServiceError::Unsupported` until V3 scheduling lands.

use domain::batch::{BatchPlan, BatchResourceLimits, PhaseKind};
use persistence_sqlite::repositories::batch_repo::BatchRepo;
use rusqlite::Connection;
use thiserror::Error;
use transport::dto::batch::{
    BatchJobDto, BatchPhaseDto, BatchPlanDto, BatchResourceLimitsDto, BatchResumeDto,
};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BatchServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for BatchServiceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}

impl transport::ServiceErrorCategory for BatchServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Serialization(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

/// Build a `BatchPlan` from a request DTO.
pub fn create_batch_plan(dto: BatchPlanDto) -> Result<BatchPlan, BatchServiceError> {
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
) -> Result<BatchJobDto, BatchServiceError> {
    let plan = create_batch_plan(plan_dto)?;
    let plan_json = serde_json::to_string(&plan)
        .map_err(|e| BatchServiceError::Serialization(format!("Failed to serialize plan: {e}")))?;

    let batch_id = Uuid::new_v4().to_string();

    let repo = BatchRepo::new(conn);
    repo.create_job(&batch_id, case_id, label, &plan_json)?;

    for kind in &plan.phases {
        repo.upsert_phase(&batch_id, &phase_kind_to_str(kind), "queued", 0.0, 0, "[]")?;
    }

    get_batch_status(conn, &batch_id)
}

/// Retrieve the full status of a batch job, including all phases.
pub fn get_batch_status(
    conn: &Connection,
    batch_id: &str,
) -> Result<BatchJobDto, BatchServiceError> {
    let repo = BatchRepo::new(conn);

    let job_row = repo
        .get_job(batch_id)?
        .ok_or_else(|| BatchServiceError::NotFound(format!("Batch job not found: {batch_id}")))?;

    let plan: BatchPlan = serde_json::from_str(&job_row.plan_json).map_err(|e| {
        BatchServiceError::Serialization(format!("Failed to deserialize plan: {e}"))
    })?;

    let phase_rows = repo.get_phases(batch_id)?;

    let phases: Vec<BatchPhaseDto> = phase_rows
        .into_iter()
        .map(|pr| {
            let warnings: Vec<String> = serde_json::from_str(&pr.warnings_json).unwrap_or_default();
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
pub fn list_batch_jobs(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<BatchJobDto>, BatchServiceError> {
    let repo = BatchRepo::new(conn);
    let rows = repo.list_jobs(case_id)?;
    rows.into_iter()
        .map(|row| get_batch_status(conn, &row.id))
        .collect()
}

// --- Stubs (MVP: create_plan + get_status are real; start/pause/resume are stubs) ---

/// Start a queued batch job.  MVP stub.
pub fn start_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, BatchServiceError> {
    // TODO: spawn async task that iterates phases, setting status to running/completed.
    Err(BatchServiceError::Unsupported(
        "batch start is not yet implemented (MVP stub)".to_string(),
    ))
}

/// Pause a running batch job.  MVP stub.
pub fn pause_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, BatchServiceError> {
    // TODO: signal the running task to pause at the next checkpoint boundary.
    Err(BatchServiceError::Unsupported(
        "batch pause is not yet implemented (MVP stub)".to_string(),
    ))
}

/// Resume a paused batch job.  MVP stub.
pub fn resume_batch(
    _conn: &Connection,
    _resume: BatchResumeDto,
) -> Result<BatchJobDto, BatchServiceError> {
    // TODO: restart the task from the last checkpoint of the paused phase.
    Err(BatchServiceError::Unsupported(
        "batch resume is not yet implemented (MVP stub)".to_string(),
    ))
}

/// Cancel a batch job (queued or running).  MVP stub.
pub fn cancel_batch(_conn: &Connection, _batch_id: &str) -> Result<BatchJobDto, BatchServiceError> {
    // TODO: signal cancellation and roll back to last checkpoint.
    Err(BatchServiceError::Unsupported(
        "batch cancel is not yet implemented (MVP stub)".to_string(),
    ))
}

// --- helpers ---

fn parse_phase_kind(s: &str) -> Result<PhaseKind, BatchServiceError> {
    match s {
        "Mount" => Ok(PhaseKind::Mount),
        "Catalog" => Ok(PhaseKind::Catalog),
        "ExtractArtifacts" => Ok(PhaseKind::ExtractArtifacts),
        "Index" => Ok(PhaseKind::Index),
        "Correlate" => Ok(PhaseKind::Correlate),
        "Export" => Ok(PhaseKind::Export),
        other => Err(BatchServiceError::InvalidInput(format!(
            "Unknown phase kind: {other}"
        ))),
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
#[path = "../tests/unit/batch_service.rs"]
mod tests;
