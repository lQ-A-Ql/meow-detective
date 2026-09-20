use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::STRICT_RBD_REPLICA_COUNT;

/// The source of truth used to decide how many RBD replicas must be present.
///
/// A directory containing N OSDs is not evidence that an RBD pool has N
/// replicas.  The strict legacy policy is therefore the only policy that can
/// be selected without a parsed pool/OSD map or equivalent trusted artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RbdReplicaPolicy {
    StrictLegacy,
    TrustedPool(PoolReplicaEvidence),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolReplicaEvidence {
    pool_id: i64,
    size: u32,
    min_size: u32,
    source: String,
    epoch: Option<u64>,
    evidence_digest: Option<String>,
}

impl RbdReplicaPolicy {
    pub fn strict_legacy() -> Self {
        Self::StrictLegacy
    }

    pub fn trusted_pool(
        pool_id: i64,
        size: u32,
        min_size: u32,
        source: impl Into<String>,
        epoch: Option<u64>,
        evidence_digest: Option<String>,
    ) -> Result<Self, ReplicaPolicyError> {
        Ok(Self::TrustedPool(PoolReplicaEvidence::new(
            pool_id,
            size,
            min_size,
            source,
            epoch,
            evidence_digest,
        )?))
    }

    pub fn expected_count(&self) -> usize {
        match self {
            Self::StrictLegacy => STRICT_RBD_REPLICA_COUNT,
            Self::TrustedPool(evidence) => evidence.size as usize,
        }
    }

    pub fn validate(&self) -> Result<(), ReplicaPolicyError> {
        match self {
            Self::StrictLegacy => Ok(()),
            Self::TrustedPool(evidence) => evidence.validate(),
        }
    }

    pub fn validate_for_pool(&self, pool_id: i64) -> Result<(), ReplicaPolicyError> {
        self.validate()?;
        if let Self::TrustedPool(evidence) = self {
            if evidence.pool_id != pool_id {
                return Err(ReplicaPolicyError::PoolMismatch);
            }
        }
        Ok(())
    }

    pub fn storage_key(&self) -> &'static str {
        match self {
            Self::StrictLegacy => "strict_rbd_replica_count",
            Self::TrustedPool(_) => "trusted_pool_size",
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            Self::StrictLegacy => "legacy_default",
            Self::TrustedPool(_) => "pool_map_or_trusted_artifact",
        }
    }

    pub fn pool_evidence(&self) -> Option<&PoolReplicaEvidence> {
        match self {
            Self::StrictLegacy => None,
            Self::TrustedPool(evidence) => Some(evidence),
        }
    }
}

impl PoolReplicaEvidence {
    fn new(
        pool_id: i64,
        size: u32,
        min_size: u32,
        source: impl Into<String>,
        epoch: Option<u64>,
        evidence_digest: Option<String>,
    ) -> Result<Self, ReplicaPolicyError> {
        let evidence = Self {
            pool_id,
            size,
            min_size,
            source: source.into().trim().to_string(),
            epoch,
            evidence_digest: evidence_digest
                .map(|digest| digest.trim().to_ascii_lowercase())
                .filter(|digest| !digest.is_empty()),
        };
        evidence.validate()?;
        Ok(evidence)
    }

    fn validate(&self) -> Result<(), ReplicaPolicyError> {
        if self.pool_id < 0
            || self.size == 0
            || self.min_size == 0
            || self.min_size > self.size
            || self.source.is_empty()
            || self.source.contains(['/', '\\', '\0'])
        {
            return Err(ReplicaPolicyError::InvalidPoolEvidence);
        }
        if self.evidence_digest.as_deref().is_some_and(|digest| {
            digest.len() != 64 || !digest.chars().all(|ch| ch.is_ascii_hexdigit())
        }) {
            return Err(ReplicaPolicyError::InvalidPoolEvidence);
        }
        Ok(())
    }

    pub fn pool_id(&self) -> i64 {
        self.pool_id
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn min_size(&self) -> u32 {
        self.min_size
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn epoch(&self) -> Option<u64> {
        self.epoch
    }

    pub fn evidence_digest(&self) -> Option<&str> {
        self.evidence_digest.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplicaPolicyError {
    #[error("invalid trusted pool replica evidence")]
    InvalidPoolEvidence,
    #[error("trusted pool evidence does not match the RBD data pool")]
    PoolMismatch,
}

/// Identity recovered from one source-local BlueStore/OSD inventory.
/// Older imports may contain only an inventory id; those entries remain
/// indeterminate instead of being treated as proof of cluster membership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaIdentity {
    pub osd_id: Option<u32>,
    pub osd_uuid: Option<String>,
    pub ceph_fsid: Option<String>,
}

impl ReplicaIdentity {
    pub fn from_inventory(
        osd_id: Option<u32>,
        osd_uuid: impl Into<String>,
        ceph_fsid: Option<String>,
    ) -> Self {
        Self {
            osd_id,
            osd_uuid: non_empty(osd_uuid.into()),
            ceph_fsid: ceph_fsid.and_then(non_empty),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.osd_id.is_some() && self.osd_uuid.is_some() && self.ceph_fsid.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryCoverageState {
    Complete,
    Incomplete,
    Conflicted,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEvidence {
    pub source_id: String,
    pub inventory_id: String,
    pub identity: ReplicaIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryCoverageReport {
    #[serde(default = "RbdReplicaPolicy::strict_legacy")]
    pub policy: RbdReplicaPolicy,
    pub expected_count: usize,
    pub observed_count: usize,
    pub state: InventoryCoverageState,
    pub duplicate_inventory_ids: Vec<String>,
    pub duplicate_source_ids: Vec<String>,
    pub duplicate_osd_ids: Vec<u32>,
    pub ceph_fsids: Vec<String>,
    pub diagnostics: Vec<String>,
}

impl InventoryCoverageReport {
    pub fn is_conflicted(&self) -> bool {
        self.state == InventoryCoverageState::Conflicted
    }

    pub fn policy_source(&self) -> &'static str {
        self.policy.source()
    }
}

pub fn assess_inventory_coverage(
    evidence: &[InventoryEvidence],
    policy: &RbdReplicaPolicy,
) -> InventoryCoverageReport {
    let expected_count = policy.expected_count();
    let inventory_ids = evidence
        .iter()
        .map(|item| item.inventory_id.as_str())
        .collect::<Vec<_>>();
    let source_ids = evidence
        .iter()
        .map(|item| item.source_id.as_str())
        .collect::<Vec<_>>();
    let osd_ids = evidence
        .iter()
        .filter_map(|item| item.identity.osd_id)
        .collect::<Vec<_>>();
    let fsids = evidence
        .iter()
        .filter_map(|item| item.identity.ceph_fsid.as_deref())
        .collect::<BTreeSet<_>>();

    let duplicate_inventory_ids = duplicate_strings(&inventory_ids);
    let duplicate_source_ids = duplicate_strings(&source_ids);
    let duplicate_osd_ids = duplicate_numbers(&osd_ids);
    let ceph_fsids = fsids.iter().map(|value| (*value).to_string()).collect();
    let mut diagnostics = Vec::new();
    if evidence.len() != expected_count {
        diagnostics.push(format!(
            "inventory coverage is not closed: expected {expected_count}, observed {}",
            evidence.len()
        ));
    }
    if !duplicate_inventory_ids.is_empty() {
        diagnostics.push("inventory identities are duplicated".to_string());
    }
    if !duplicate_source_ids.is_empty() {
        diagnostics.push("data-source identities are duplicated".to_string());
    }
    if !duplicate_osd_ids.is_empty() {
        diagnostics.push("OSD identities are duplicated".to_string());
    }
    if fsids.len() > 1 {
        diagnostics.push("Ceph FSIDs conflict across inventory entries".to_string());
    }
    if evidence.iter().any(|item| !item.identity.is_complete()) {
        diagnostics.push("one or more inventory entries lack complete OSD identity".to_string());
    }

    let state = if !duplicate_inventory_ids.is_empty()
        || !duplicate_source_ids.is_empty()
        || !duplicate_osd_ids.is_empty()
        || fsids.len() > 1
    {
        InventoryCoverageState::Conflicted
    } else if evidence.len() != expected_count {
        InventoryCoverageState::Incomplete
    } else if evidence.iter().all(|item| item.identity.is_complete()) {
        InventoryCoverageState::Complete
    } else {
        InventoryCoverageState::Indeterminate
    };

    InventoryCoverageReport {
        policy: policy.clone(),
        expected_count,
        observed_count: evidence.len(),
        state,
        duplicate_inventory_ids,
        duplicate_source_ids,
        duplicate_osd_ids,
        ceph_fsids,
        diagnostics,
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn duplicate_strings(values: &[&str]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for value in values {
        if !seen.insert(*value) {
            duplicates.insert((*value).to_string());
        }
    }
    duplicates.into_iter().collect()
}

fn duplicate_numbers(values: &[u32]) -> Vec<u32> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for value in values {
        if !seen.insert(*value) {
            duplicates.insert(*value);
        }
    }
    duplicates.into_iter().collect()
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/cluster_evidence.rs"]
mod tests;
