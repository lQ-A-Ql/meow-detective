use domain::DataSourceKind;
use std::path::PathBuf;
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum ClusterServiceError {
    #[error("cluster parsing is planned but not implemented in this milestone")]
    Unsupported,
    #[error("at least two evidence sources are required for cluster parsing")]
    InsufficientSources,
}

impl transport::ServiceErrorCategory for ClusterServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Unsupported => transport::ErrorCategory::Unsupported,
            Self::InsufficientSources => transport::ErrorCategory::Validation,
        }
    }
}

pub type Result<T> = std::result::Result<T, ClusterServiceError>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_parse_plan_is_explicitly_non_executing() {
        let plan = plan_cluster_parse(ClusterParseRequest {
            sources: vec![
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-a.E01"),
                    source_kind: DataSourceKind::E01,
                },
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-b.E01"),
                    source_kind: DataSourceKind::E01,
                },
            ],
        })
        .unwrap();

        assert_eq!(plan.source_count, 2);
        assert!(!plan.supported_now);
        assert_eq!(
            plan.boundary,
            ClusterParseBoundary::PlannedPveLvmThinCluster
        );
    }

    #[test]
    fn cluster_parse_execution_is_unsupported_in_single_disk_milestone() {
        let err = parse_cluster(ClusterParseRequest {
            sources: vec![
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-a.E01"),
                    source_kind: DataSourceKind::E01,
                },
                ClusterEvidenceSource {
                    source_path: PathBuf::from("node-b.E01"),
                    source_kind: DataSourceKind::E01,
                },
            ],
        })
        .unwrap_err();

        assert!(matches!(err, ClusterServiceError::Unsupported));
    }

    #[test]
    fn cluster_parse_plan_requires_multiple_sources() {
        let err = plan_cluster_parse(ClusterParseRequest {
            sources: vec![ClusterEvidenceSource {
                source_path: PathBuf::from("single.E01"),
                source_kind: DataSourceKind::E01,
            }],
        })
        .unwrap_err();

        assert!(matches!(err, ClusterServiceError::InsufficientSources));
    }
}
