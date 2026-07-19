use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::types::{
    CephFsDescriptor, CephFsDescriptorState, CephFsMapEvidence, CephFsMapProvenance,
    CephFsPoolBinding, CephFsPoolEvidence, CephFsPoolProvenance, CephFsPoolRole, CephFsRankBinding,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsBindingError {
    #[error("CephFS map evidence is empty")]
    NoMapEvidence,
    #[error("CephFS {field} is empty or contains a NUL byte")]
    InvalidIdentity { field: &'static str },
    #[error("CephFS {field} is not a canonical SHA-256 digest")]
    InvalidSnapshotDigest { field: &'static str },
    #[error("CephFS map evidence has no raw MDSMap snapshot for filesystem {filesystem_id}")]
    MissingMdsMapSnapshot { filesystem_id: i64 },
    #[error("CephFS raw MDSMap snapshot set does not match the decoded FSMap")]
    MdsMapSnapshotSetMismatch,
    #[error(
        "CephFS source '{source_identity}' inventory '{inventory_identity}' has conflicting snapshots"
    )]
    ConflictingSourceSnapshot {
        source_identity: String,
        inventory_identity: String,
    },
    #[error(
        "CephFS cluster identity conflict: expected '{expected}', observed '{observed}' from source '{source_identity}'"
    )]
    ConflictingClusterIdentity {
        expected: String,
        observed: String,
        source_identity: String,
    },
    #[error(
        "CephFS FSMap conflict from source '{source_identity}': expected epoch {expected_epoch}, observed {observed_epoch}"
    )]
    ConflictingFsMap {
        source_identity: String,
        expected_epoch: u32,
        observed_epoch: u32,
    },
    #[error("CephFS pool {pool_id} has no inventory evidence")]
    MissingPoolBinding { pool_id: i64 },
    #[error("CephFS MDS rank {rank} references missing daemon GID {gid}")]
    MissingRankDaemon { rank: i32, gid: u64 },
    #[error("CephFS MDS rank {rank} does not match daemon GID {gid} rank {daemon_rank}")]
    RankDaemonMismatch {
        rank: i32,
        gid: u64,
        daemon_rank: i32,
    },
    #[error("CephFS MDS rank {rank} cannot be represented as an unsigned rank")]
    InvalidRank { rank: i32 },
    #[error(
        "CephFS pool {pool_id} belongs to cluster '{observed}', expected '{expected}' (source '{source_identity}')"
    )]
    PoolClusterMismatch {
        pool_id: i64,
        expected: String,
        observed: String,
        source_identity: String,
    },
}

impl transport::ServiceErrorCategory for CephFsBindingError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::NoMapEvidence
            | Self::InvalidIdentity { .. }
            | Self::InvalidSnapshotDigest { .. }
            | Self::MissingMdsMapSnapshot { .. }
            | Self::MdsMapSnapshotSetMismatch => transport::ErrorCategory::Validation,
            Self::ConflictingSourceSnapshot { .. }
            | Self::ConflictingClusterIdentity { .. }
            | Self::ConflictingFsMap { .. }
            | Self::MissingPoolBinding { .. }
            | Self::MissingRankDaemon { .. }
            | Self::RankDaemonMismatch { .. }
            | Self::InvalidRank { .. }
            | Self::PoolClusterMismatch { .. } => transport::ErrorCategory::Parser,
        }
    }
}

pub fn bind_cephfs_descriptors(
    map_evidence: &[CephFsMapEvidence],
    pool_evidence: &[CephFsPoolEvidence],
) -> Result<Vec<CephFsDescriptor>, CephFsBindingError> {
    let maps = canonical_map_evidence(map_evidence)?;
    let first = maps.first().ok_or(CephFsBindingError::NoMapEvidence)?;
    validate_map_consistency(&maps, first)?;
    let pools = canonical_pool_evidence(pool_evidence, &first.cluster_identity)?;
    build_descriptors(first, &maps, &pools)
}

fn canonical_map_evidence(
    evidence: &[CephFsMapEvidence],
) -> Result<Vec<CephFsMapEvidence>, CephFsBindingError> {
    let mut canonical = BTreeMap::new();
    for item in evidence {
        validate_identity(&item.cluster_identity, "cluster identity")?;
        validate_identity(&item.source_identity, "source identity")?;
        validate_identity(&item.inventory_identity, "inventory identity")?;
        validate_map_snapshot_digests(item)?;
        let key = (
            item.source_identity.clone(),
            item.inventory_identity.clone(),
        );
        if let Some(existing) = canonical.get(&key) {
            if existing != item {
                return Err(CephFsBindingError::ConflictingSourceSnapshot {
                    source_identity: item.source_identity.clone(),
                    inventory_identity: item.inventory_identity.clone(),
                });
            }
        } else {
            canonical.insert(key, item.clone());
        }
    }
    Ok(canonical.into_values().collect())
}

fn validate_map_consistency(
    maps: &[CephFsMapEvidence],
    expected: &CephFsMapEvidence,
) -> Result<(), CephFsBindingError> {
    for item in maps.iter().skip(1) {
        if item.cluster_identity != expected.cluster_identity {
            return Err(CephFsBindingError::ConflictingClusterIdentity {
                expected: expected.cluster_identity.clone(),
                observed: item.cluster_identity.clone(),
                source_identity: item.source_identity.clone(),
            });
        }
        if item.map != expected.map
            || item.raw_fsmap_sha256 != expected.raw_fsmap_sha256
            || item.raw_mdsmap_sha256 != expected.raw_mdsmap_sha256
        {
            return Err(CephFsBindingError::ConflictingFsMap {
                source_identity: item.source_identity.clone(),
                expected_epoch: expected.map.epoch,
                observed_epoch: item.map.epoch,
            });
        }
    }
    Ok(())
}

fn canonical_pool_evidence(
    evidence: &[CephFsPoolEvidence],
    cluster_identity: &str,
) -> Result<BTreeMap<i64, Vec<CephFsPoolProvenance>>, CephFsBindingError> {
    let mut canonical = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for item in evidence {
        validate_identity(&item.source_identity, "pool source identity")?;
        validate_identity(&item.inventory_identity, "pool inventory identity")?;
        if item.cluster_identity != cluster_identity {
            return Err(CephFsBindingError::PoolClusterMismatch {
                pool_id: item.pool_id,
                expected: cluster_identity.to_string(),
                observed: item.cluster_identity.clone(),
                source_identity: item.source_identity.clone(),
            });
        }
        let key = (
            item.pool_id,
            item.source_identity.clone(),
            item.inventory_identity.clone(),
        );
        if !identities.insert(key) {
            continue;
        }
        canonical
            .entry(item.pool_id)
            .or_insert_with(BTreeSet::new)
            .insert(CephFsPoolProvenance {
                source_identity: item.source_identity.clone(),
                inventory_identity: item.inventory_identity.clone(),
            });
    }
    Ok(canonical
        .into_iter()
        .map(|(pool_id, provenance)| (pool_id, provenance.into_iter().collect()))
        .collect())
}

fn build_descriptors(
    source: &CephFsMapEvidence,
    maps: &[CephFsMapEvidence],
    pools: &BTreeMap<i64, Vec<CephFsPoolProvenance>>,
) -> Result<Vec<CephFsDescriptor>, CephFsBindingError> {
    let mut descriptors = Vec::with_capacity(source.map.filesystems.len());
    for filesystem in &source.map.filesystems {
        let provenance = maps
            .iter()
            .map(|item| {
                let raw_mdsmap_sha256 = item
                    .raw_mdsmap_sha256
                    .get(&filesystem.filesystem_id)
                    .cloned()
                    .ok_or(CephFsBindingError::MissingMdsMapSnapshot {
                        filesystem_id: filesystem.filesystem_id,
                    })?;
                Ok(CephFsMapProvenance {
                    source_identity: item.source_identity.clone(),
                    inventory_identity: item.inventory_identity.clone(),
                    captured_at: item.captured_at,
                    raw_fsmap_sha256: item.raw_fsmap_sha256.clone(),
                    raw_mdsmap_sha256,
                })
            })
            .collect::<Result<Vec<_>, CephFsBindingError>>()?;
        let rank_bindings = bind_rank_daemons(&filesystem.mds_map)?;
        let metadata_pool = bind_pool(
            filesystem.mds_map.metadata_pool_id,
            CephFsPoolRole::Metadata,
            pools,
        )?;
        let mut data_pools = Vec::with_capacity(filesystem.mds_map.data_pool_ids.len());
        for (ordinal, pool_id) in filesystem.mds_map.data_pool_ids.iter().enumerate() {
            data_pools.push(bind_pool(
                *pool_id,
                CephFsPoolRole::Data {
                    ordinal: ordinal as u32,
                },
                pools,
            )?);
        }
        descriptors.push(CephFsDescriptor {
            identity: format!(
                "ceph-fs:{}:{}:{}:{}",
                source.cluster_identity,
                filesystem.filesystem_id,
                source.map.epoch,
                filesystem.mds_map.metadata_pool_id
            ),
            cluster_identity: source.cluster_identity.clone(),
            filesystem_id: filesystem.filesystem_id,
            name: filesystem.mds_map.name.clone(),
            fsmap_epoch: source.map.epoch,
            mdsmap_epoch: filesystem.mds_map.epoch,
            state: if filesystem
                .mds_map
                .daemons
                .iter()
                .any(|daemon| daemon.state.is_active())
            {
                CephFsDescriptorState::Present
            } else {
                CephFsDescriptorState::PresentButNotReplayable
            },
            metadata_pool,
            data_pools,
            rank_bindings,
            daemons: filesystem.mds_map.daemons.clone(),
            provenance: provenance.clone(),
        });
    }
    Ok(descriptors)
}

fn bind_rank_daemons(
    map: &ceph_wire::CephMdsMap,
) -> Result<Vec<CephFsRankBinding>, CephFsBindingError> {
    let daemons = map
        .daemons
        .iter()
        .map(|daemon| (daemon.gid, daemon))
        .collect::<BTreeMap<_, _>>();
    map.up_ranks
        .iter()
        .map(|(rank, gid)| {
            let daemon = daemons
                .get(gid)
                .ok_or(CephFsBindingError::MissingRankDaemon {
                    rank: *rank,
                    gid: *gid,
                })?;
            if daemon.rank != *rank {
                return Err(CephFsBindingError::RankDaemonMismatch {
                    rank: *rank,
                    gid: *gid,
                    daemon_rank: daemon.rank,
                });
            }
            Ok(CephFsRankBinding {
                rank: u32::try_from(*rank)
                    .map_err(|_| CephFsBindingError::InvalidRank { rank: *rank })?,
                gid: *gid,
                incarnation: daemon.incarnation,
            })
        })
        .collect()
}

fn bind_pool(
    pool_id: i64,
    role: CephFsPoolRole,
    pools: &BTreeMap<i64, Vec<CephFsPoolProvenance>>,
) -> Result<CephFsPoolBinding, CephFsBindingError> {
    let provenance = pools
        .get(&pool_id)
        .cloned()
        .ok_or(CephFsBindingError::MissingPoolBinding { pool_id })?;
    Ok(CephFsPoolBinding {
        pool_id,
        role,
        provenance,
    })
}

fn validate_identity(value: &str, field: &'static str) -> Result<(), CephFsBindingError> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(CephFsBindingError::InvalidIdentity { field });
    }
    Ok(())
}

fn validate_map_snapshot_digests(item: &CephFsMapEvidence) -> Result<(), CephFsBindingError> {
    validate_sha256(&item.raw_fsmap_sha256, "raw FSMap snapshot digest")?;
    if item.raw_mdsmap_sha256.len() != item.map.filesystems.len() {
        return Err(CephFsBindingError::MdsMapSnapshotSetMismatch);
    }
    for filesystem in &item.map.filesystems {
        let digest = item
            .raw_mdsmap_sha256
            .get(&filesystem.filesystem_id)
            .ok_or(CephFsBindingError::MissingMdsMapSnapshot {
                filesystem_id: filesystem.filesystem_id,
            })?;
        validate_sha256(digest, "raw MDSMap snapshot digest")?;
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &'static str) -> Result<(), CephFsBindingError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CephFsBindingError::InvalidSnapshotDigest { field });
    }
    Ok(())
}
