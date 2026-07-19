use std::collections::{BTreeMap, BTreeSet};

use app_services::ceph_reconstruction::{
    bind_cephfs_descriptors, persist_cephfs_journal_replay as persist_with_map_evidence,
    CephFsDescriptor, CephFsJournalPersistenceError, CephFsJournalPersistenceOutcome,
    CephFsJournalReplay, CephFsJournalStopReason, CephFsMapEvidence, CephFsPoolEvidence,
};
use ceph_wire::{CephFsFilesystem, CephFsMap, CephMdsDaemon, CephMdsMap, CephMdsState};
use chrono::{TimeZone, Utc};
use persistence_sqlite::repositories::ceph_fs_journal_repo::CephFsJournalRepo;

#[path = "support/cephfs_journal_persistence.rs"]
mod support;

fn persist_cephfs_journal_replay(
    conn: &rusqlite::Connection,
    replay: &CephFsJournalReplay,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<CephFsJournalPersistenceOutcome, CephFsJournalPersistenceError> {
    persist_with_map_evidence(
        conn,
        replay,
        &map_descriptor(),
        data_source_id,
        inventory_id,
    )
}

fn map_descriptor() -> CephFsDescriptor {
    let mds_map = CephMdsMap {
        epoch: 23,
        name: "cephfs-a".to_string(),
        enabled: true,
        metadata_pool_id: 7,
        data_pool_ids: Vec::new(),
        max_mds: 1,
        last_failure_osd_epoch: 16,
        daemons: vec![CephMdsDaemon {
            gid: 123,
            name: "mds-a".to_string(),
            rank: 0,
            incarnation: 4,
            state: CephMdsState::Active,
            state_sequence: 99,
        }],
        in_ranks: BTreeSet::from([0]),
        up_ranks: BTreeMap::from([(0, 123)]),
        failed_ranks: BTreeSet::new(),
        stopped_ranks: BTreeSet::new(),
        damaged_ranks: BTreeSet::new(),
    };
    let map = CephFsMap {
        epoch: 17,
        filesystems: vec![CephFsFilesystem {
            filesystem_id: 1,
            mds_map,
        }],
    };
    let maps = ["monitor-a", "monitor-b"].map(|source| CephFsMapEvidence {
        cluster_identity: "cluster-a".to_string(),
        source_identity: source.to_string(),
        inventory_identity: format!("map-17-{source}"),
        captured_at: Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, 0).unwrap(),
        raw_fsmap_sha256: "a".repeat(64),
        raw_mdsmap_sha256: BTreeMap::from([(1, "b".repeat(64))]),
        map: map.clone(),
    });
    bind_cephfs_descriptors(
        &maps,
        &[CephFsPoolEvidence {
            pool_id: 7,
            cluster_identity: "cluster-a".to_string(),
            source_identity: "osd-a".to_string(),
            inventory_identity: "inventory-osd-a".to_string(),
        }],
    )
    .expect("bind map evidence")
    .remove(0)
}

#[test]
fn source_local_replay_projections_are_isolated_deterministic_and_payload_free() {
    let (source_a_db, source_a) = support::setup_source("source-a", "inventory-a", '1');
    let (source_b_db, source_b) = support::setup_source("source-b", "inventory-b", '4');
    let replay = support::replay_fixture(&[source_a.clone(), source_b.clone()], false);

    assert_eq!(
        persist_cephfs_journal_replay(
            &source_a_db,
            &replay,
            &source_a.source_id,
            &source_a.inventory_id,
        )
        .unwrap(),
        CephFsJournalPersistenceOutcome::Replaced
    );
    assert_eq!(
        persist_cephfs_journal_replay(
            &source_b_db,
            &replay,
            &source_b.source_id,
            &source_b.inventory_id,
        )
        .unwrap(),
        CephFsJournalPersistenceOutcome::Replaced
    );
    assert_eq!(
        persist_cephfs_journal_replay(
            &source_a_db,
            &replay,
            &source_a.source_id,
            &source_a.inventory_id,
        )
        .unwrap(),
        CephFsJournalPersistenceOutcome::Unchanged
    );

    let projection_a = CephFsJournalRepo::new(&source_a_db)
        .find(support::FILESYSTEM, &source_a.inventory_id, 0)
        .unwrap()
        .unwrap();
    let projection_b = CephFsJournalRepo::new(&source_b_db)
        .find(support::FILESYSTEM, &source_b.inventory_id, 0)
        .unwrap()
        .unwrap();
    assert_eq!(
        projection_a.manifest.pointer_object_identity_sha256,
        source_a.pointer
    );
    assert_eq!(
        projection_b.manifest.pointer_object_identity_sha256,
        source_b.pointer
    );
    assert_eq!(projection_a.spans[0].object_identity_sha256, source_a.data);
    assert_eq!(projection_b.spans[0].object_identity_sha256, source_b.data);
    assert_ne!(
        projection_a.manifest.metadata_inventory_sha256,
        projection_b.manifest.metadata_inventory_sha256
    );
    assert_eq!(
        projection_a.manifest.consensus_replay_sha256,
        replay.replay_sha256
    );
    assert_eq!(
        projection_b.manifest.consensus_replay_sha256,
        replay.replay_sha256
    );
    assert_ne!(
        projection_a.manifest.projection_sha256,
        projection_b.manifest.projection_sha256
    );
    assert_eq!(projection_a.map_provenance.len(), 2);
    assert_eq!(
        projection_a.manifest.raw_fsmap_snapshot_sha256,
        "a".repeat(64)
    );

    for conn in [&source_a_db, &source_b_db] {
        let raw_payload_columns: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ceph_fs_journal_events')
                 WHERE lower(name) LIKE '%raw%' OR lower(name) LIKE '%payload_bytes%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw_payload_columns, 0);
        let blob_columns: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ceph_fs_journal_events')
                 WHERE upper(type) = 'BLOB'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blob_columns, 0);
    }
}

#[test]
fn missing_local_provenance_fails_before_any_projection_write() {
    let (_source_a_db, source_a) = support::setup_source("source-a", "inventory-a", '1');
    let (source_b_db, source_b) = support::setup_source("source-b", "inventory-b", '4');
    let replay_without_b = support::replay_fixture(std::slice::from_ref(&source_a), false);
    assert!(matches!(
        persist_cephfs_journal_replay(
            &source_b_db,
            &replay_without_b,
            &source_b.source_id,
            &source_b.inventory_id,
        ),
        Err(CephFsJournalPersistenceError::MissingLocalProvenance { .. })
    ));
    assert!(CephFsJournalRepo::new(&source_b_db)
        .find(support::FILESYSTEM, &source_b.inventory_id, 0)
        .unwrap()
        .is_none());
}

#[test]
fn duplicate_reader_provenance_is_rejected_before_frame_acceptance_and_audited() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let replay = support::replay_fixture(std::slice::from_ref(&source), true);

    assert_eq!(
        replay.stop_reason,
        Some(CephFsJournalStopReason::ResponseMismatch)
    );
    assert!(replay.events.is_empty());
    assert_eq!(
        persist_cephfs_journal_replay(
            &source_db,
            &replay,
            &source.source_id,
            &source.inventory_id,
        )
        .unwrap(),
        CephFsJournalPersistenceOutcome::Replaced
    );
    let stored = CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.manifest.stop_reason.as_deref(),
        Some("response_mismatch")
    );
    assert!(stored.events.is_empty());
    assert!(stored.spans.is_empty());
}

#[test]
fn non_initial_lid_is_persisted_as_ignored_without_freezing_sequence() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let replay = support::replay_fixture_with_non_initial_lid(std::slice::from_ref(&source));

    assert_eq!(
        replay.events[1].sequence_status.as_str(),
        "ignored_non_initial_lid"
    );
    persist_cephfs_journal_replay(&source_db, &replay, &source.source_id, &source.inventory_id)
        .unwrap();
    let stored = CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .unwrap();
    assert_eq!(stored.events[1].sequence_disposition, "ignored_lid");
    assert_eq!(stored.events[1].segment_sequence_hex, None);
    assert_eq!(stored.events[1].event_sequence_hex, None);
}

#[test]
fn zero_length_event_payload_retains_physical_frame_audit() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let replay = support::replay_fixture_with_empty_payload(std::slice::from_ref(&source));

    assert_eq!(replay.events.len(), 1);
    assert_eq!(replay.events[0].frame.payload_length, 0);
    persist_cephfs_journal_replay(&source_db, &replay, &source.source_id, &source.inventory_id)
        .unwrap();
    let stored = CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .unwrap();
    assert_eq!(stored.events[0].payload_length, 0);
    assert_eq!(
        stored.events[0].sequence_disposition,
        "semantic_unavailable"
    );
    assert_eq!(
        stored.manifest.sequence_stop_reason.as_deref(),
        Some("unsupported_semantics")
    );
}

#[test]
fn control_provenance_tampering_invalidates_the_in_memory_replay_digest() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let mut replay = support::replay_fixture(std::slice::from_ref(&source), false);
    replay.pointer_spans[0].range_sha256 = "f".repeat(64);

    assert_eq!(
        persist_cephfs_journal_replay(&source_db, &replay, &source.source_id, &source.inventory_id,),
        Err(CephFsJournalPersistenceError::ReplayDigestMismatch)
    );
    assert!(CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .is_none());
}

#[test]
fn stale_metadata_inventory_digest_is_rejected_before_replay_write() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let replay = support::replay_fixture(std::slice::from_ref(&source), false);
    source_db
        .execute(
            "UPDATE ceph_fs_metadata_inventories SET object_count = object_count + 1
             WHERE filesystem_identity = ?1 AND inventory_id = ?2",
            rusqlite::params![support::FILESYSTEM, source.inventory_id],
        )
        .unwrap();

    assert_eq!(
        persist_cephfs_journal_replay(&source_db, &replay, &source.source_id, &source.inventory_id,),
        Err(CephFsJournalPersistenceError::MetadataInventoryUnavailable)
    );
    assert!(CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .is_none());
}

#[test]
fn tampered_map_evidence_fails_before_projection_write() {
    let (source_db, source) = support::setup_source("source-a", "inventory-a", '1');
    let replay = support::replay_fixture(std::slice::from_ref(&source), false);
    let mut descriptor = map_descriptor();
    descriptor.provenance[0].raw_fsmap_sha256 = "f".repeat(64);

    assert_eq!(
        persist_with_map_evidence(
            &source_db,
            &replay,
            &descriptor,
            &source.source_id,
            &source.inventory_id,
        ),
        Err(CephFsJournalPersistenceError::InvalidSourceBinding)
    );
    assert!(CephFsJournalRepo::new(&source_db)
        .find(support::FILESYSTEM, &source.inventory_id, 0)
        .unwrap()
        .is_none());
}
