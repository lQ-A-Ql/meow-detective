use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{RbdReplicaPolicy, ReplicaIdentity};
use crate::cluster_service::scope_storage;

const EVIDENCE_FILE: &str = "osdmap-evidence.json";
const EVIDENCE_KIND: &str = "ceph_osdmap_poolmap";
const SCHEMA_VERSION: u32 = 2;
const DIGEST_DOMAIN: &[u8] = b"meow-detective-ceph-osdmap-poolmap-v2\0";
const BINDING_DIGEST_DOMAIN: &[u8] = b"meow-detective-ceph-map-binding-v1\0";
const MAX_EVIDENCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_POOL_RECORDS: usize = 16_384;
const MAX_OSD_RECORDS: usize = 131_072;
const MAX_EPOCH_HISTORY: usize = 4_096;

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
    #[error("OSDMap evidence does not prove target-pool PG acting sets")]
    PlacementNotProven,
    #[error("OSDMap evidence does not contain the requested pool")]
    PoolNotFound,
    #[error("imported OSD identity is not present in the OSDMap")]
    ReplicaNotInMap,
    #[error("imported OSD identity conflicts with the OSDMap")]
    ReplicaIdentityMismatch,
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
    source: EvidenceSource,
    osdmap_payload_digest: String,
    poolmap_payload_digest: String,
    map_binding_digest: String,
    epoch_history: Vec<EpochBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceSource {
    source_kind: String,
    source_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EpochBinding {
    epoch: u64,
    osdmap_payload_digest: String,
    poolmap_payload_digest: String,
    map_binding_digest: String,
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
    let path = evidence_path(case_root, cluster_id, EVIDENCE_FILE);
    let payload = match read_evidence_payload(&path)? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    let mut document: EvidenceDocument =
        serde_json::from_slice(&payload).map_err(|_| OsdMapEvidenceError::Json)?;
    validate_document(&document, cluster_id)?;
    let provided_digest = document.evidence_digest.clone();
    let expected_digest = canonical_digest(&mut document)?;
    if provided_digest != expected_digest {
        return Err(OsdMapEvidenceError::DigestMismatch);
    }
    let _pool = match pool_id {
        Some(pool_id) => document
            .pools
            .iter()
            .find(|pool| pool.pool_id == pool_id)
            .ok_or(OsdMapEvidenceError::PoolNotFound)?,
        None if document.pools.len() == 1 => &document.pools[0],
        None => return Err(OsdMapEvidenceError::Invalid("pool binding")),
    };
    Err(OsdMapEvidenceError::PlacementNotProven)
}

pub(crate) fn validate_inventory_membership(
    case_root: &Path,
    cluster_id: &str,
    identities: &[ReplicaIdentity],
) -> Result<Option<usize>, OsdMapEvidenceError> {
    validate_cluster_id(cluster_id)?;
    let path = evidence_path(case_root, cluster_id, EVIDENCE_FILE);
    let payload = match read_evidence_payload(&path)? {
        Some(payload) => payload,
        None => return Ok(None),
    };
    let mut document: EvidenceDocument =
        serde_json::from_slice(&payload).map_err(|_| OsdMapEvidenceError::Json)?;
    validate_document(&document, cluster_id)?;
    let provided_digest = document.evidence_digest.clone();
    if canonical_digest(&mut document)? != provided_digest {
        return Err(OsdMapEvidenceError::DigestMismatch);
    }
    for identity in identities {
        let osd_id = identity
            .osd_id
            .ok_or(OsdMapEvidenceError::ReplicaIdentityMismatch)?;
        let expected = document
            .osds
            .iter()
            .find(|osd| osd.osd_id == osd_id)
            .ok_or(OsdMapEvidenceError::ReplicaNotInMap)?;
        let Some(osd_uuid) = identity.osd_uuid.as_deref() else {
            return Err(OsdMapEvidenceError::ReplicaIdentityMismatch);
        };
        let Some(ceph_fsid) = identity.ceph_fsid.as_deref() else {
            return Err(OsdMapEvidenceError::ReplicaIdentityMismatch);
        };
        if Uuid::parse_str(osd_uuid).ok() != Uuid::parse_str(&expected.osd_uuid).ok()
            || ceph_fsid != document.ceph_fsid
            || ceph_fsid != expected.ceph_fsid
        {
            return Err(OsdMapEvidenceError::ReplicaIdentityMismatch);
        }
    }
    Ok(Some(document.osds.len()))
}

pub(crate) fn evidence_is_present(
    case_root: &Path,
    cluster_id: &str,
) -> Result<bool, OsdMapEvidenceError> {
    validate_cluster_id(cluster_id)?;
    let path = evidence_path(case_root, cluster_id, EVIDENCE_FILE);
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(OsdMapEvidenceError::Read),
    }
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
    if document.epoch == 0
        || document.pools.is_empty()
        || document.pools.len() > MAX_POOL_RECORDS
        || document.osds.is_empty()
        || document.osds.len() > MAX_OSD_RECORDS
    {
        return Err(OsdMapEvidenceError::Invalid("map completeness"));
    }
    validate_pools(&document.pools)?;
    validate_osds(&document.osds, &document.ceph_fsid)?;
    validate_digest(&document.evidence_digest, "evidence digest")?;
    validate_v2_contract(document)?;
    Ok(())
}

fn validate_v2_contract(document: &EvidenceDocument) -> Result<(), OsdMapEvidenceError> {
    validate_source(&document.source)?;
    let osdmap_digest = &document.osdmap_payload_digest;
    let poolmap_digest = &document.poolmap_payload_digest;
    let binding_digest = &document.map_binding_digest;
    validate_digest(osdmap_digest, "OSDMap payload digest")?;
    validate_digest(poolmap_digest, "PoolMap payload digest")?;
    validate_digest(binding_digest, "map binding digest")?;
    if binding_digest_for(document, document.epoch, osdmap_digest, poolmap_digest)
        != *binding_digest
    {
        return Err(OsdMapEvidenceError::Invalid("map binding digest"));
    }
    let history = &document.epoch_history;
    if history.is_empty() || history.len() > MAX_EPOCH_HISTORY {
        return Err(OsdMapEvidenceError::Invalid("epoch history"));
    }
    let mut previous_epoch = None;
    for entry in history {
        if entry.epoch == 0 || previous_epoch.is_some_and(|previous| entry.epoch <= previous) {
            return Err(OsdMapEvidenceError::Invalid("epoch monotonicity"));
        }
        validate_digest(&entry.osdmap_payload_digest, "OSDMap payload digest")?;
        validate_digest(&entry.poolmap_payload_digest, "PoolMap payload digest")?;
        validate_digest(&entry.map_binding_digest, "map binding digest")?;
        if binding_digest_for(
            document,
            entry.epoch,
            &entry.osdmap_payload_digest,
            &entry.poolmap_payload_digest,
        ) != entry.map_binding_digest
        {
            return Err(OsdMapEvidenceError::Invalid("map binding digest"));
        }
        previous_epoch = Some(entry.epoch);
    }
    let current = history
        .last()
        .ok_or(OsdMapEvidenceError::Invalid("epoch history"))?;
    if current.epoch != document.epoch
        || current.osdmap_payload_digest != *osdmap_digest
        || current.poolmap_payload_digest != *poolmap_digest
        || current.map_binding_digest != *binding_digest
    {
        return Err(OsdMapEvidenceError::Invalid("epoch binding"));
    }
    Ok(())
}

fn validate_source(source: &EvidenceSource) -> Result<(), OsdMapEvidenceError> {
    if source.source_kind.is_empty()
        || source.source_kind.len() > 64
        || !source.source_kind.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        || source.source_id.is_empty()
        || source.source_id.len() > 256
        || source
            .source_id
            .chars()
            .any(|character| character.is_control())
    {
        return Err(OsdMapEvidenceError::Invalid("source provenance"));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), OsdMapEvidenceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(OsdMapEvidenceError::Invalid(field));
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
        let Ok(osd_uuid) = Uuid::parse_str(&osd.osd_uuid) else {
            return Err(OsdMapEvidenceError::Invalid("OSD identity"));
        };
        if !uuids.insert(osd_uuid) {
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

pub(crate) fn evidence_path(case_root: &Path, scope_id: &str, file_name: &str) -> PathBuf {
    scope_storage::artifact_path(case_root, scope_id, "ceph-evidence", file_name)
}

fn read_evidence_payload(path: &Path) -> Result<Option<Vec<u8>>, OsdMapEvidenceError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OsdMapEvidenceError::Read),
    };
    if !metadata.is_file() {
        return Err(OsdMapEvidenceError::Read);
    }
    if metadata.len() > MAX_EVIDENCE_BYTES {
        return Err(OsdMapEvidenceError::Invalid("evidence size"));
    }
    let file = std::fs::File::open(path).map_err(|_| OsdMapEvidenceError::Read)?;
    let mut payload = Vec::new();
    let read_limit = MAX_EVIDENCE_BYTES
        .checked_add(1)
        .ok_or(OsdMapEvidenceError::Invalid("evidence size"))?;
    file.take(read_limit)
        .read_to_end(&mut payload)
        .map_err(|_| OsdMapEvidenceError::Read)?;
    if payload.len() as u64 > MAX_EVIDENCE_BYTES {
        return Err(OsdMapEvidenceError::Invalid("evidence size"));
    }
    Ok(Some(payload))
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

fn binding_digest_for(
    document: &EvidenceDocument,
    epoch: u64,
    osdmap_digest: &str,
    poolmap_digest: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(BINDING_DIGEST_DOMAIN);
    digest.update(document.ceph_fsid.as_bytes());
    digest.update([0]);
    digest.update(document.ceph_revision.as_bytes());
    digest.update([0]);
    digest.update(epoch.to_le_bytes());
    digest.update([0]);
    digest.update(osdmap_digest.as_bytes());
    digest.update([0]);
    digest.update(poolmap_digest.as_bytes());
    hex::encode(digest.finalize())
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/osdmap_evidence.rs"]
mod tests;
