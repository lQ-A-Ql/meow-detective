use std::collections::{BTreeMap, BTreeSet};

use app_services::ceph_reconstruction::{
    bind_cephfs_descriptors, CephFsBindingError, CephFsDescriptorState, CephFsMapEvidence,
    CephFsPoolEvidence, CephFsPoolRole,
};
use ceph_wire::{CephFsFilesystem, CephFsMap, CephMdsDaemon, CephMdsMap, CephMdsState};
use chrono::{TimeZone, Utc};

fn mds_map(state: CephMdsState) -> CephMdsMap {
    CephMdsMap {
        epoch: 17,
        name: "cephfs-a".to_string(),
        enabled: true,
        metadata_pool_id: 3,
        data_pool_ids: vec![5, 7],
        max_mds: 1,
        last_failure_osd_epoch: 16,
        daemons: vec![CephMdsDaemon {
            gid: 123,
            name: "mds-a".to_string(),
            rank: 0,
            incarnation: 4,
            state,
            state_sequence: 99,
        }],
        in_ranks: BTreeSet::from([0]),
        up_ranks: BTreeMap::from([(0, 123)]),
        failed_ranks: BTreeSet::new(),
        stopped_ranks: BTreeSet::new(),
        damaged_ranks: BTreeSet::new(),
    }
}

fn map(state: CephMdsState) -> CephFsMap {
    CephFsMap {
        epoch: 17,
        filesystems: vec![CephFsFilesystem {
            filesystem_id: 1,
            mds_map: mds_map(state),
        }],
    }
}

fn map_evidence(source: &str, inventory: &str, state: CephMdsState) -> CephFsMapEvidence {
    CephFsMapEvidence {
        cluster_identity: "cluster-a".to_string(),
        source_identity: source.to_string(),
        inventory_identity: inventory.to_string(),
        captured_at: Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, 0).unwrap(),
        raw_fsmap_sha256: "a".repeat(64),
        raw_mdsmap_sha256: BTreeMap::from([(1, "b".repeat(64))]),
        map: map(state),
    }
}

fn pool_evidence(pool_id: i64, source: &str) -> CephFsPoolEvidence {
    CephFsPoolEvidence {
        pool_id,
        cluster_identity: "cluster-a".to_string(),
        source_identity: source.to_string(),
        inventory_identity: format!("inventory-{source}"),
    }
}

fn all_pool_evidence() -> Vec<CephFsPoolEvidence> {
    [3, 5, 7]
        .into_iter()
        .flat_map(|pool_id| {
            [
                pool_evidence(pool_id, "osd-a"),
                pool_evidence(pool_id, "osd-b"),
            ]
        })
        .collect()
}

#[test]
fn binds_consistent_cross_source_maps_and_pool_provenance() {
    let maps = vec![
        map_evidence("monitor-a", "map-17-a", CephMdsState::Active),
        map_evidence("monitor-b", "map-17-b", CephMdsState::Active),
    ];
    let descriptors = bind_cephfs_descriptors(&maps, &all_pool_evidence()).expect("bind maps");

    assert_eq!(descriptors.len(), 1);
    let descriptor = &descriptors[0];
    assert_eq!(descriptor.identity, "ceph-fs:cluster-a:1:17:3");
    assert_eq!(descriptor.state, CephFsDescriptorState::Present);
    assert_eq!(descriptor.provenance.len(), 2);
    assert_eq!(descriptor.provenance[0].raw_fsmap_sha256, "a".repeat(64));
    assert_eq!(descriptor.provenance[0].raw_mdsmap_sha256, "b".repeat(64));
    assert_eq!(descriptor.rank_bindings[0].incarnation, 4);
    assert_eq!(descriptor.metadata_pool.pool_id, 3);
    assert_eq!(descriptor.metadata_pool.role, CephFsPoolRole::Metadata);
    assert_eq!(descriptor.metadata_pool.provenance.len(), 2);
    assert_eq!(descriptor.data_pools.len(), 2);
    assert_eq!(
        descriptor.data_pools[1].role,
        CephFsPoolRole::Data { ordinal: 1 }
    );
}

#[test]
fn duplicate_source_import_is_idempotent() {
    let item = map_evidence("monitor-a", "map-17-a", CephMdsState::Active);
    let pools = all_pool_evidence();
    let expected = bind_cephfs_descriptors(std::slice::from_ref(&item), &pools).unwrap();
    let observed = bind_cephfs_descriptors(&[item.clone(), item], &[pools.clone(), pools].concat())
        .expect("deduplicate identical evidence");
    assert_eq!(observed, expected);
}

#[test]
fn no_active_mds_preserves_present_but_not_replayable_state() {
    let descriptors = bind_cephfs_descriptors(
        &[map_evidence("monitor-a", "map-17-a", CephMdsState::Replay)],
        &all_pool_evidence(),
    )
    .unwrap();
    assert_eq!(
        descriptors[0].state,
        CephFsDescriptorState::PresentButNotReplayable
    );
}

#[test]
fn cross_source_map_conflicts_fail_closed() {
    let first = map_evidence("monitor-a", "map-17-a", CephMdsState::Active);
    let mut conflicting = map_evidence("monitor-b", "map-18-b", CephMdsState::Active);
    conflicting.map.epoch = 18;
    conflicting.map.filesystems[0].mds_map.epoch = 18;
    assert_eq!(
        bind_cephfs_descriptors(&[first, conflicting], &all_pool_evidence()).unwrap_err(),
        CephFsBindingError::ConflictingFsMap {
            source_identity: "monitor-b".to_string(),
            expected_epoch: 17,
            observed_epoch: 18,
        }
    );
}

#[test]
fn pool_binding_requires_complete_same_cluster_evidence() {
    let maps = [map_evidence("monitor-a", "map-17-a", CephMdsState::Active)];
    let missing_data_pool = vec![pool_evidence(3, "osd-a"), pool_evidence(5, "osd-a")];
    assert_eq!(
        bind_cephfs_descriptors(&maps, &missing_data_pool).unwrap_err(),
        CephFsBindingError::MissingPoolBinding { pool_id: 7 }
    );

    let mut wrong_cluster = all_pool_evidence();
    wrong_cluster[0].cluster_identity = "cluster-b".to_string();
    assert!(matches!(
        bind_cephfs_descriptors(&maps, &wrong_cluster),
        Err(CephFsBindingError::PoolClusterMismatch { pool_id: 3, .. })
    ));
}

#[test]
fn conflicting_duplicate_snapshot_identity_fails_closed() {
    let first = map_evidence("monitor-a", "map-17-a", CephMdsState::Active);
    let mut duplicate = first.clone();
    duplicate.captured_at = Utc.with_ymd_and_hms(2026, 7, 19, 11, 0, 0).unwrap();
    assert!(matches!(
        bind_cephfs_descriptors(&[first, duplicate], &all_pool_evidence()),
        Err(CephFsBindingError::ConflictingSourceSnapshot { .. })
    ));
}

#[test]
fn raw_map_snapshot_digest_conflicts_fail_closed() {
    let first = map_evidence("monitor-a", "map-17-a", CephMdsState::Active);
    let mut conflicting = map_evidence("monitor-b", "map-17-b", CephMdsState::Active);
    conflicting.raw_mdsmap_sha256.insert(1, "c".repeat(64));
    assert!(matches!(
        bind_cephfs_descriptors(&[first, conflicting], &all_pool_evidence()),
        Err(CephFsBindingError::ConflictingFsMap { .. })
    ));
}
