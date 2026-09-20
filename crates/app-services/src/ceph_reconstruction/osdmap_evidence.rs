use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::RbdReplicaPolicy;

const EVIDENCE_FILE: &str = "osdmap-evidence.json";
const EVIDENCE_KIND: &str = "ceph_osdmap_poolmap";
const SCHEMA_VERSION: u32 = 1;
const DIGEST_DOMAIN: &[u8] = b"meow-detective-ceph-osdmap-poolmap-v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum OsdMapEvidenceError {
    #[error("OSDMap evidence could not be read")]
    Read,
    #[error("OSDMap evidence is not valid JSON")]
    Json,
    #[error("OSDMap evidence has an invalid {0}")]
    Invalid(&'static str),
    #[error("OSDMap evidence contains duplicate {0}")]
    Duplicate(&'static str),
    #[error("OSDMap evidence digest does not match its canonical content")]
    DigestMismatch,
    #[error("OSDMap evidence contains an unsupported pool type")]
    UnsupportedPoolType,
    #[error("OSDMap evidence does not contain the requested pool")]
    PoolNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OsdMapPolicyResolution {
    pub(crate) policy: RbdReplicaPolicy,
    pub(crate) osd_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceDocument {
    schema_version: u32,
    evidence_kind: String,
    cluster_id: String,
    ceph_fsid: String,
    ceph_revision: String,
    epoch: u64,
    pools: Vec<PoolRecord>,
    osds: Vec<OsdRecord>,
    evidence_digest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PoolRecord {
    pool_id: i64,
    pool_type: String,
    size: u32,
    min_size: u32,
    pg_num: u32,
    pgp_num: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OsdRecord {
    osd_id: u32,
    osd_uuid: String,
    up: bool,
    #[serde(rename = "in")]
    in_cluster: bool,
    weight: u64,
    address: String,
    ceph_fsid: String,
}

pub(crate) fn resolve_policy(
    case_root: &Path,
    cluster_id: &str,
    pool_id: Option<i64>,
) -> Result<Option<OsdMapPolicyResolution>, OsdMapEvidenceError> {
    validate_cluster_id(cluster_id)?;
    let path = case_root
        .join("clusters")
        .join(cluster_id)
        .join(EVIDENCE_FILE);
    let payload = match std::fs::read(path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OsdMapEvidenceError::Read),
    };
    let mut document: EvidenceDocument =
        serde_json::from_slice(&payload).map_err(|_| OsdMapEvidenceError::Json)?;
    validate_document(&document, cluster_id)?;
    let provided_digest = document.evidence_digest.clone();
    let expected_digest = canonical_digest(&mut document)?;
    if provided_digest != expected_digest {
        return Err(OsdMapEvidenceError::DigestMismatch);
    }
    let pool = match pool_id {
        Some(pool_id) => document
            .pools
            .iter()
            .find(|pool| pool.pool_id == pool_id)
            .ok_or(OsdMapEvidenceError::PoolNotFound)?,
        None if document.pools.len() == 1 => &document.pools[0],
        None => return Err(OsdMapEvidenceError::Invalid("pool binding")),
    };
    let source = format!("osdmap:{}:epoch-{}", document.ceph_revision, document.epoch);
    let policy = RbdReplicaPolicy::trusted_pool(
        pool.pool_id,
        pool.size,
        pool.min_size,
        source,
        Some(document.epoch),
        Some(document.evidence_digest.clone()),
    )
    .map_err(|_| OsdMapEvidenceError::Invalid("replica policy"))?;
    Ok(Some(OsdMapPolicyResolution {
        policy,
        osd_count: document.osds.len(),
    }))
}

fn validate_document(
    document: &EvidenceDocument,
    expected_cluster_id: &str,
) -> Result<(), OsdMapEvidenceError> {
    if document.schema_version != SCHEMA_VERSION {
        return Err(OsdMapEvidenceError::Invalid("schema version"));
    }
    if document.evidence_kind != EVIDENCE_KIND {
        return Err(OsdMapEvidenceError::Invalid("evidence kind"));
    }
    if document.cluster_id != expected_cluster_id {
        return Err(OsdMapEvidenceError::Invalid("cluster identity"));
    }
    if Uuid::parse_str(&document.ceph_fsid).is_err() {
        return Err(OsdMapEvidenceError::Invalid("Ceph FSID"));
    }
    validate_revision(&document.ceph_revision)?;
    if document.epoch == 0 || document.pools.is_empty() || document.osds.is_empty() {
        return Err(OsdMapEvidenceError::Invalid("map completeness"));
    }
    validate_pools(&document.pools)?;
    validate_osds(&document.osds, &document.ceph_fsid)?;
    if document.evidence_digest.len() != 64
        || !document
            .evidence_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(OsdMapEvidenceError::Invalid("evidence digest"));
    }
    Ok(())
}

fn validate_pools(pools: &[PoolRecord]) -> Result<(), OsdMapEvidenceError> {
    let mut ids = BTreeSet::new();
    for pool in pools {
        if pool.pool_id < 0 {
            return Err(OsdMapEvidenceError::Invalid("pool ID"));
        }
        if !ids.insert(pool.pool_id) {
            return Err(OsdMapEvidenceError::Duplicate("pool IDs"));
        }
        if pool.pool_type != "replicated" {
            return Err(OsdMapEvidenceError::UnsupportedPoolType);
        }
        if pool.size == 0 || pool.min_size == 0 || pool.min_size > pool.size {
            return Err(OsdMapEvidenceError::Invalid("pool replica geometry"));
        }
        if pool.pg_num == 0 || pool.pgp_num == 0 || pool.pgp_num > pool.pg_num {
            return Err(OsdMapEvidenceError::Invalid("pool PG geometry"));
        }
    }
    Ok(())
}

fn validate_osds(osds: &[OsdRecord], ceph_fsid: &str) -> Result<(), OsdMapEvidenceError> {
    let mut ids = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    for osd in osds {
        if !ids.insert(osd.osd_id) {
            return Err(OsdMapEvidenceError::Duplicate("OSD IDs"));
        }
        if Uuid::parse_str(&osd.osd_uuid).is_err() || !uuids.insert(osd.osd_uuid.as_str()) {
            return Err(OsdMapEvidenceError::Duplicate("OSD UUIDs"));
        }
        if osd.ceph_fsid != ceph_fsid || osd.address.trim().is_empty() {
            return Err(OsdMapEvidenceError::Invalid("OSD identity"));
        }
    }
    Ok(())
}

fn validate_revision(revision: &str) -> Result<(), OsdMapEvidenceError> {
    if revision.is_empty()
        || revision.len() > 128
        || revision
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(OsdMapEvidenceError::Invalid("Ceph revision"));
    }
    Ok(())
}

fn validate_cluster_id(cluster_id: &str) -> Result<(), OsdMapEvidenceError> {
    if cluster_id.is_empty()
        || cluster_id == "."
        || cluster_id == ".."
        || cluster_id.contains(['/', '\\'])
    {
        return Err(OsdMapEvidenceError::Invalid("cluster ID"));
    }
    Ok(())
}

fn canonical_digest(document: &mut EvidenceDocument) -> Result<String, OsdMapEvidenceError> {
    document.pools.sort_by_key(|pool| pool.pool_id);
    document.osds.sort_by_key(|osd| osd.osd_id);
    document.evidence_digest.clear();
    let bytes = serde_json::to_vec(document).map_err(|_| OsdMapEvidenceError::Json)?;
    let mut digest = Sha256::new();
    digest.update(DIGEST_DOMAIN);
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/osdmap_evidence.rs"]
mod tests;
