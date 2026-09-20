use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{RbdReplicaPolicy, ReplicaPolicyError};

const POOL_EVIDENCE_SCHEMA_VERSION: u32 = 1;
const POOL_EVIDENCE_KIND: &str = "rbd_pool_replication";
const POOL_EVIDENCE_FILE: &str = "pool-evidence.json";

/// The result of resolving a cluster's pool replication evidence.
///
/// Diagnostics are intentionally semantic and do not contain host paths or
/// raw evidence payloads. They are suitable for the coverage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaPolicyResolution {
    pub policy: RbdReplicaPolicy,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Error)]
pub enum PoolEvidenceError {
    #[error("pool replication evidence could not be read")]
    Read,
    #[error("pool replication evidence is not valid JSON")]
    Json,
    #[error("pool replication evidence has an invalid {0}")]
    Invalid(&'static str),
    #[error("pool replication evidence contains duplicate pool IDs")]
    DuplicatePool,
    #[error("trusted pool evidence is inconsistent with the imported OSD set")]
    ReplicaCountMismatch,
    #[error("trusted pool evidence is invalid: {0}")]
    Policy(#[from] ReplicaPolicyError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolEvidenceDocument {
    schema_version: u32,
    cluster_id: String,
    evidence_kind: String,
    pools: Vec<PoolEvidenceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolEvidenceRecord {
    pool_id: i64,
    size: u32,
    min_size: u32,
    source: String,
    epoch: Option<u64>,
    evidence_digest: Option<String>,
}

/// Resolve an explicit pool evidence document for a cluster.
///
/// A missing document is a supported legacy state and falls back to the
/// strict three-replica policy. A present but malformed document is rejected;
/// silently falling back would turn tampered evidence into a valid-looking
/// reconstruction. When no pool is known yet, a single-record document is
/// unambiguous; multiple records remain strict until a descriptor binds one.
pub(crate) fn resolve_rbd_replica_policy(
    case_root: &Path,
    cluster_id: &str,
    pool_id: Option<i64>,
    observed_replica_count: usize,
) -> Result<ReplicaPolicyResolution, PoolEvidenceError> {
    validate_cluster_id(cluster_id)?;
    let path = case_root
        .join("clusters")
        .join(cluster_id)
        .join(POOL_EVIDENCE_FILE);
    let payload = match std::fs::read(&path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(strict_fallback("pool replication evidence unavailable"));
        }
        Err(_) => return Err(PoolEvidenceError::Read),
    };
    let document: PoolEvidenceDocument =
        serde_json::from_slice(&payload).map_err(|_| PoolEvidenceError::Json)?;
    validate_document(&document, cluster_id)?;
    let selected = select_record(&document.pools, pool_id);
    let Some(record) = selected else {
        return Ok(strict_fallback(match pool_id {
            Some(pool) => return_missing_pool_diagnostic(pool),
            None => "multiple pool evidence records require an RBD pool binding",
        }));
    };
    if record.size as usize != observed_replica_count {
        return Err(PoolEvidenceError::ReplicaCountMismatch);
    }
    let policy = RbdReplicaPolicy::trusted_pool(
        record.pool_id,
        record.size,
        record.min_size,
        record.source.clone(),
        record.epoch,
        record.evidence_digest.clone(),
    )?;
    Ok(ReplicaPolicyResolution {
        policy,
        diagnostics: Vec::new(),
    })
}

fn validate_cluster_id(cluster_id: &str) -> Result<(), PoolEvidenceError> {
    if cluster_id.is_empty()
        || cluster_id == "."
        || cluster_id == ".."
        || cluster_id.contains(['/', '\\'])
    {
        return Err(PoolEvidenceError::Invalid("cluster ID"));
    }
    Ok(())
}

fn validate_document(
    document: &PoolEvidenceDocument,
    expected_cluster_id: &str,
) -> Result<(), PoolEvidenceError> {
    if document.schema_version != POOL_EVIDENCE_SCHEMA_VERSION {
        return Err(PoolEvidenceError::Invalid("schema version"));
    }
    if document.cluster_id != expected_cluster_id {
        return Err(PoolEvidenceError::Invalid("cluster identity"));
    }
    if document.evidence_kind != POOL_EVIDENCE_KIND {
        return Err(PoolEvidenceError::Invalid("evidence kind"));
    }
    if document.pools.is_empty() {
        return Err(PoolEvidenceError::Invalid("pool records"));
    }
    let mut pool_ids = BTreeSet::new();
    for record in &document.pools {
        if !pool_ids.insert(record.pool_id) {
            return Err(PoolEvidenceError::DuplicatePool);
        }
        RbdReplicaPolicy::trusted_pool(
            record.pool_id,
            record.size,
            record.min_size,
            record.source.clone(),
            record.epoch,
            record.evidence_digest.clone(),
        )?;
    }
    Ok(())
}

fn select_record(
    records: &[PoolEvidenceRecord],
    pool_id: Option<i64>,
) -> Option<&PoolEvidenceRecord> {
    match pool_id {
        Some(pool_id) => records.iter().find(|record| record.pool_id == pool_id),
        None if records.len() == 1 => records.first(),
        None => None,
    }
}

fn strict_fallback(reason: &str) -> ReplicaPolicyResolution {
    ReplicaPolicyResolution {
        policy: RbdReplicaPolicy::strict_legacy(),
        diagnostics: vec![format!(
            "{reason}; fallback to strict legacy replica policy"
        )],
    }
}

fn return_missing_pool_diagnostic(pool_id: i64) -> &'static str {
    if pool_id < 0 {
        "requested RBD pool has no trusted replication evidence"
    } else {
        "requested RBD pool is absent from trusted replication evidence"
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/pool_evidence.rs"]
mod tests;
