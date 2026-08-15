use std::path::PathBuf;

use domain::DataSourceKind;

use super::{ClusterServiceError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterParseRequest {
    pub sources: Vec<ClusterEvidenceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterEvidenceSource {
    pub source_path: PathBuf,
    pub source_kind: DataSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterParsePlan {
    pub source_count: usize,
    pub supported_now: bool,
    pub boundary: ClusterParseBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterParseBoundary {
    PlannedPveLvmThinCluster,
}

/// Reserved entry point for future PVE / multi-source cluster parsing.
///
/// This milestone intentionally remains focused on single-disk XFS-on-LVM
/// parsing. Keeping a typed service boundary prevents later cluster work from
/// being mixed into datasource probing or the single-disk import path.
pub fn plan_cluster_parse(request: ClusterParseRequest) -> Result<ClusterParsePlan> {
    if request.sources.len() < 2 {
        return Err(ClusterServiceError::InsufficientSources);
    }

    Ok(ClusterParsePlan {
        source_count: request.sources.len(),
        supported_now: false,
        boundary: ClusterParseBoundary::PlannedPveLvmThinCluster,
    })
}

pub fn parse_cluster(_request: ClusterParseRequest) -> Result<()> {
    Err(ClusterServiceError::Unsupported)
}
