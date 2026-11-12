use std::collections::{BTreeMap, BTreeSet};

use super::cephfs_presence::{
    CephFsFilesystemPresenceRecord, CephFsMapPresenceSnapshot, CephFsMdsFilesystemPresenceRecord,
    CephFsMdsMapPresenceSnapshot, CephFsPresenceDiagnostic,
};

pub(super) fn validate_unique_filesystems(
    filesystems: &[CephFsFilesystemPresenceRecord],
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for filesystem in filesystems {
        if !seen.insert(filesystem.filesystem_id) {
            diagnostics.push(CephFsPresenceDiagnostic::InvalidFilesystemBinding {
                filesystem_id: filesystem.filesystem_id,
                reason: "FSMap contains a duplicate filesystem ID".to_string(),
            });
        }
        let unique_data_pools = filesystem
            .data_pool_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if filesystem.metadata_pool_id == 0
            || filesystem.data_pool_ids.is_empty()
            || unique_data_pools.len() != filesystem.data_pool_ids.len()
            || unique_data_pools.contains(&filesystem.metadata_pool_id)
        {
            diagnostics.push(CephFsPresenceDiagnostic::InvalidFilesystemBinding {
                filesystem_id: filesystem.filesystem_id,
                reason: "filesystem has missing, duplicate, or overlapping pool bindings"
                    .to_string(),
            });
        }
        if filesystem.data_pool_ids.contains(&0) {
            diagnostics.push(CephFsPresenceDiagnostic::InvalidFilesystemBinding {
                filesystem_id: filesystem.filesystem_id,
                reason: "filesystem contains an invalid data pool ID".to_string(),
            });
        }
    }
}

pub(super) fn validate_unique_mds_filesystems(
    source_id: &str,
    filesystems: &[CephFsMdsFilesystemPresenceRecord],
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for filesystem in filesystems {
        if !seen.insert(filesystem.filesystem_id) {
            diagnostics.push(CephFsPresenceDiagnostic::FsmapMdsmapMismatch {
                source_id: source_id.to_string(),
                reason: "MDSMap contains a duplicate filesystem ID".to_string(),
            });
        }
    }
}

pub(super) fn canonical_filesystem_ids(ids: impl IntoIterator<Item = u64>) -> Vec<u64> {
    let mut ids = ids.into_iter().collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(super) fn canonical_filesystems(
    filesystems: &[CephFsFilesystemPresenceRecord],
) -> Vec<CephFsFilesystemPresenceRecord> {
    let mut filesystems = filesystems.to_vec();
    for filesystem in &mut filesystems {
        filesystem.data_pool_ids.sort_unstable();
    }
    filesystems.sort_by_key(|filesystem| filesystem.filesystem_id);
    filesystems
}

pub(super) fn validate_filesystem_bindings(
    fsmap: &CephFsMapPresenceSnapshot,
    mdsmap: &CephFsMdsMapPresenceSnapshot,
    diagnostics: &mut Vec<CephFsPresenceDiagnostic>,
) {
    let mds_by_id = mdsmap
        .filesystems
        .iter()
        .map(|filesystem| (filesystem.filesystem_id, filesystem.rank_count))
        .collect::<BTreeMap<_, _>>();
    for filesystem in &fsmap.filesystems {
        if !mds_by_id.contains_key(&filesystem.filesystem_id) {
            diagnostics.push(CephFsPresenceDiagnostic::MissingMdsBinding {
                filesystem_id: filesystem.filesystem_id,
            });
        }
    }
    if mds_by_id.len() != fsmap.filesystems.len() {
        diagnostics.push(CephFsPresenceDiagnostic::FsmapMdsmapMismatch {
            source_id: fsmap.source_identity.clone(),
            reason: "FSMap and MDSMap filesystem sets differ".to_string(),
        });
    }
}
