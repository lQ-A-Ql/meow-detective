use std::{path::Path, sync::atomic::AtomicBool};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CaseProjectionBuildReport {
    pub graph_rebuilt: bool,
    pub correlation_rebuilt: bool,
    pub infrastructure_rebuilt: bool,
    pub completed_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CaseProjectionError {
    #[error("case graph projection failed: {0}")]
    Graph(#[from] crate::graph_service::GraphServiceError),
    #[error("correlation projection failed: {0}")]
    Correlation(#[from] crate::correlation::CorrelationError),
    #[error("infrastructure projection failed: {0}")]
    Infrastructure(#[from] crate::infrastructure_graph_service::InfrastructureProjectionError),
    #[error("case projection build cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseProjectionJobOutcome {
    Completed(String),
    Cancelled,
    Failed(String),
}

impl transport::ServiceErrorCategory for CaseProjectionError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Graph(error) => error.category(),
            Self::Correlation(error) => error.category(),
            Self::Infrastructure(error) => error.category(),
            Self::Cancelled => transport::ErrorCategory::Cancelled,
        }
    }
}

pub fn rebuild_case_projections(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &str,
    cancel_token: &AtomicBool,
) -> Result<CaseProjectionBuildReport, CaseProjectionError> {
    ensure_not_cancelled(cancel_token)?;
    crate::correlation::rebuild_correlation_snapshot_for_case(
        case_conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
        cancel_token,
    )?;
    ensure_not_cancelled(cancel_token)?;
    crate::infrastructure_graph_service::rebuild_infrastructure_graph_for_case(
        case_conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
        cancel_token,
    )?;
    ensure_not_cancelled(cancel_token)?;
    crate::graph_service::rebuild_case_graph_for_case(case_conn, case_root, case_id, cancel_token)?;
    Ok(CaseProjectionBuildReport {
        graph_rebuilt: true,
        correlation_rebuilt: true,
        infrastructure_rebuilt: true,
        completed_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn run_case_projection_job(
    connection: &rusqlite::Connection,
    case_root: &Path,
    case_id: &str,
    job_id: &domain::JobId,
    cancel_token: &AtomicBool,
) -> Result<CaseProjectionJobOutcome, crate::job_service::JobServiceError> {
    let repo = persistence_sqlite::repositories::job_repo::JobRepo::new(connection);
    if !repo.start(job_id)? {
        return Ok(CaseProjectionJobOutcome::Cancelled);
    }
    repo.update_progress(job_id, 5, "Building case projections")?;
    match rebuild_case_projections(connection, case_root, case_id, cancel_token) {
        Ok(report) => {
            let detail = format!("Case projections ready at {}", report.completed_at);
            repo.complete_committed(job_id, &detail)?;
            Ok(CaseProjectionJobOutcome::Completed(detail))
        }
        Err(CaseProjectionError::Cancelled) => {
            repo.cancel(job_id, "Case projection build cancelled")?;
            Ok(CaseProjectionJobOutcome::Cancelled)
        }
        Err(error) => {
            let detail = error.to_string();
            repo.fail(job_id, &detail)?;
            Ok(CaseProjectionJobOutcome::Failed(detail))
        }
    }
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), CaseProjectionError> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(CaseProjectionError::Cancelled)
    } else {
        Ok(())
    }
}
