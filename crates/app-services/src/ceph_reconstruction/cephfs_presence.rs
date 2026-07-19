use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::{
    datasource_cluster_repo::DataSourceClusterRepo, datasource_repo::DataSourceRepo,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    cephfs_presence_storage::read_presence_evidence, cephfs_presence_validation::assess_presence,
};
use crate::source_db::open_reconstruction_source_by_id;

pub const FSMAP_PRESENCE_KEY: &str = "ceph.fsmap.presence.v1";
pub const MDSMAP_PRESENCE_KEY: &str = "ceph.mdsmap.presence.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CephFsPresenceState {
    Present,
    Absent,
    Indeterminate,
}

impl std::fmt::Display for CephFsPresenceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Present => "present",
            Self::Absent => "absent",
            Self::Indeterminate => "indeterminate",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CephFsPresenceMapKind {
    Fsmap,
    Mdsmap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CephFsPresenceDiagnostic {
    NoSourceEvidence,
    SourceSetIncomplete {
        expected: usize,
        observed: usize,
    },
    SourceUnavailable {
        source_id: String,
        reason: String,
    },
    MissingSnapshot {
        source_id: String,
        map: CephFsPresenceMapKind,
    },
    MalformedSnapshot {
        source_id: String,
        map: CephFsPresenceMapKind,
        reason: String,
    },
    FreshnessUnproven {
        source_id: String,
        map: CephFsPresenceMapKind,
        reason: String,
    },
    SnapshotIdentityMismatch {
        source_id: String,
        map: CephFsPresenceMapKind,
        reason: String,
    },
    ConflictingClusterIdentity {
        source_id: String,
        expected: String,
        observed: String,
    },
    ConflictingMapEpoch {
        source_id: String,
        map: CephFsPresenceMapKind,
        expected: u64,
        observed: u64,
    },
    ConflictingFilesystemSet {
        source_id: String,
        map: CephFsPresenceMapKind,
        expected: Vec<u64>,
        observed: Vec<u64>,
    },
    FsmapMdsmapMismatch {
        source_id: String,
        reason: String,
    },
    MissingMdsBinding {
        filesystem_id: u64,
    },
    InvalidFilesystemBinding {
        filesystem_id: u64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsPresenceAssessment {
    pub state: CephFsPresenceState,
    pub source_count: usize,
    pub filesystem_count: usize,
    pub fsmap_epoch: Option<u64>,
    pub mdsmap_epoch: Option<u64>,
    pub diagnostics: Vec<CephFsPresenceDiagnostic>,
}

impl CephFsPresenceAssessment {
    fn force_indeterminate(&mut self, diagnostic: CephFsPresenceDiagnostic) {
        self.state = CephFsPresenceState::Indeterminate;
        self.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsPresenceEvidence {
    pub source_id: String,
    pub fsmap: Option<CephFsMapPresenceSnapshot>,
    pub mdsmap: Option<CephFsMdsMapPresenceSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fsmap_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdsmap_error: Option<String>,
}

impl CephFsPresenceEvidence {
    pub fn new(
        source_id: impl Into<String>,
        fsmap: Option<CephFsMapPresenceSnapshot>,
        mdsmap: Option<CephFsMdsMapPresenceSnapshot>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            fsmap,
            mdsmap,
            fsmap_error: None,
            mdsmap_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsMapPresenceSnapshot {
    pub schema_version: u32,
    pub cluster_identity: String,
    pub source_identity: String,
    pub inventory_identity: String,
    pub epoch: u64,
    pub captured_at: String,
    pub filesystems: Vec<CephFsFilesystemPresenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsMdsMapPresenceSnapshot {
    pub schema_version: u32,
    pub cluster_identity: String,
    pub source_identity: String,
    pub inventory_identity: String,
    pub fsmap_epoch: u64,
    pub epoch: u64,
    pub captured_at: String,
    pub filesystems: Vec<CephFsMdsFilesystemPresenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsFilesystemPresenceRecord {
    pub filesystem_id: u64,
    pub metadata_pool_id: u64,
    pub data_pool_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CephFsMdsFilesystemPresenceRecord {
    pub filesystem_id: u64,
    pub rank_count: u32,
}

#[derive(Debug, Error)]
pub enum CephFsPresenceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("cluster '{0}' was not found")]
    ClusterNotFound(String),
    #[error("cluster '{cluster_id}' does not belong to case '{case_id}'")]
    ClusterCaseMismatch { cluster_id: String, case_id: String },
}

impl transport::ServiceErrorCategory for CephFsPresenceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(error) => error.category(),
            Self::ClusterNotFound(_) | Self::ClusterCaseMismatch { .. } => {
                transport::ErrorCategory::Validation
            }
        }
    }
}

pub fn assess_cephfs_presence_for_cluster(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) -> Result<CephFsPresenceAssessment, CephFsPresenceError> {
    let cluster = DataSourceClusterRepo::new(case_conn)
        .find_by_id(cluster_id)?
        .ok_or_else(|| CephFsPresenceError::ClusterNotFound(cluster_id.to_string()))?;
    if cluster.case_id != *case_id {
        return Err(CephFsPresenceError::ClusterCaseMismatch {
            cluster_id: cluster_id.to_string(),
            case_id: case_id.0.clone(),
        });
    }

    let source_ids = DataSourceRepo::new(case_conn).find_ids_by_cluster(case_id, cluster_id)?;
    let mut evidence = Vec::with_capacity(source_ids.len());
    let mut source_diagnostics = Vec::new();
    for data_source_id in source_ids {
        match open_reconstruction_source_by_id(case_conn, case_root, case_id, &data_source_id) {
            Ok(source) => {
                evidence.push(read_presence_evidence(&data_source_id, &source.connection)?)
            }
            Err(error) => {
                source_diagnostics.push(CephFsPresenceDiagnostic::SourceUnavailable {
                    source_id: data_source_id.0,
                    reason: error.to_string(),
                });
            }
        }
    }

    let mut evaluated = assess_cephfs_presence(&evidence, cluster.member_count as usize);
    if cluster.import_state != "ready" {
        evaluated.force_indeterminate(CephFsPresenceDiagnostic::SourceSetIncomplete {
            expected: cluster.member_count as usize,
            observed: cluster.ready_count as usize,
        });
    }
    evaluated.diagnostics.extend(source_diagnostics);
    if evaluated.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::SourceUnavailable { .. }
        )
    }) {
        evaluated.state = CephFsPresenceState::Indeterminate;
    }
    Ok(evaluated)
}

pub fn assess_cephfs_presence(
    evidence: &[CephFsPresenceEvidence],
    expected_source_count: usize,
) -> CephFsPresenceAssessment {
    assess_presence(evidence, expected_source_count)
}
