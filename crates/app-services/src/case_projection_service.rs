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

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), CaseProjectionError> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(CaseProjectionError::Cancelled)
    } else {
        Ok(())
    }
}
