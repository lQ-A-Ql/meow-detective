use std::collections::BTreeMap;

use app_services::ceph_reconstruction::{
    discover_cephfs_journal_ranks, replay_cephfs_journal, CephFsDescriptor, CephFsDescriptorState,
    CephFsJournalDiscoveryError, CephFsJournalFramingStatus, CephFsJournalNamespaceStopReason,
    CephFsJournalReplayError, CephFsJournalReplayLimits, CephFsJournalStopReason,
    CephFsMergedMetadataInventory, CephFsMergedMetadataObject, CephFsObjectLocator,
    CephFsObjectMetadata, CephFsObjectRange, CephFsObjectRangeReader, CephFsObjectReadError,
    CephFsObjectReadProvenance, CephFsPoolBinding, CephFsPoolRole, CephFsRankBinding,
};
use ceph_wire::{
    format_cephfs_journal_data_object_name, format_cephfs_journal_pointer_object_name,
    plan_cephfs_journal_range, CephFsJournalLayout, CephMdsDaemon, CephMdsState,
    CEPHFS_JOURNAL_MAGIC,
};

const POOL_ID: i64 = 7;
const FSMAP_EPOCH: u32 = 17;
const RANK: u32 = 0;
const FRONT_INODE: u64 = 0x200;
const SENTINEL: u64 = 0x3141_5926_5358_9793;

struct FixtureObjectReader {
    descriptor: CephFsDescriptor,
    objects: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Copy)]
enum ResponseFault {
    Locator,
    Offset,
    Size,
    Provenance,
    DifferentProvenance,
}

struct FaultingObjectReader {
    inner: FixtureObjectReader,
    inspect_fault: Option<(usize, ResponseFault)>,
    range_fault: Option<(usize, ResponseFault)>,
    inspect_calls: usize,
    range_calls: usize,
}

impl FaultingObjectReader {
    fn inspect(inner: FixtureObjectReader, call: usize, fault: ResponseFault) -> Self {
        Self {
            inner,
            inspect_fault: Some((call, fault)),
            range_fault: None,
            inspect_calls: 0,
            range_calls: 0,
        }
    }

    fn range(inner: FixtureObjectReader, call: usize, fault: ResponseFault) -> Self {
        Self {
            inner,
            inspect_fault: None,
            range_fault: Some((call, fault)),
            inspect_calls: 0,
            range_calls: 0,
        }
    }
}

impl FixtureObjectReader {
    fn new(descriptor: CephFsDescriptor) -> Self {
        Self {
            descriptor,
            objects: BTreeMap::new(),
        }
    }

    fn insert(&mut self, object_name: String, bytes: Vec<u8>) {
        let locator = locator(&self.descriptor, object_name);
        self.objects.insert(locator.canonical(), bytes);
    }
}

impl CephFsObjectRangeReader for FixtureObjectReader {
    fn inspect_object(
        &mut self,
        locator: &CephFsObjectLocator,
    ) -> Result<CephFsObjectMetadata, CephFsObjectReadError> {
        let canonical = locator.canonical();
        let bytes =
            self.objects
                .get(&canonical)
                .ok_or_else(|| CephFsObjectReadError::ObjectNotFound {
                    locator: canonical.clone(),
                })?;
        Ok(CephFsObjectMetadata {
            filesystem_identity: self.descriptor.identity.clone(),
            locator: canonical,
            object_size: bytes.len() as u64,
            provenance: provenance(),
        })
    }

    fn read_range(
        &mut self,
        locator: &CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError> {
        let canonical = locator.canonical();
        let bytes =
            self.objects
                .get(&canonical)
                .ok_or_else(|| CephFsObjectReadError::ObjectNotFound {
                    locator: canonical.clone(),
                })?;
        let end = offset.checked_add(length as u64).ok_or_else(|| {
            CephFsObjectReadError::RangeOverflow {
                locator: canonical.clone(),
            }
        })?;
        if end > bytes.len() as u64 {
            return Err(CephFsObjectReadError::RangeOutOfBounds {
                locator: canonical,
                object_size: bytes.len() as u64,
            });
        }
        Ok(CephFsObjectRange {
            filesystem_identity: self.descriptor.identity.clone(),
            locator: locator.canonical(),
            object_size: bytes.len() as u64,
            offset,
            bytes: bytes[offset as usize..end as usize].to_vec(),
            provenance: provenance(),
        })
    }
}

impl CephFsObjectRangeReader for FaultingObjectReader {
    fn inspect_object(
        &mut self,
        locator: &CephFsObjectLocator,
    ) -> Result<CephFsObjectMetadata, CephFsObjectReadError> {
        self.inspect_calls += 1;
        let mut metadata = self.inner.inspect_object(locator)?;
        if let Some((_call, fault)) = self
            .inspect_fault
            .filter(|(call, _)| *call == self.inspect_calls)
        {
            match fault {
                ResponseFault::Locator => metadata.locator.push_str(":wrong"),
                ResponseFault::Size => metadata.object_size = 0,
                ResponseFault::Provenance => metadata.provenance.clear(),
                ResponseFault::Offset | ResponseFault::DifferentProvenance => {}
            }
        }
        Ok(metadata)
    }

    fn read_range(
        &mut self,
        locator: &CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError> {
        self.range_calls += 1;
        let mut range = self.inner.read_range(locator, offset, length)?;
        if let Some((_call, fault)) = self
            .range_fault
            .filter(|(call, _)| *call == self.range_calls)
        {
            match fault {
                ResponseFault::Locator => range.locator.push_str(":wrong"),
                ResponseFault::Offset => range.offset = range.offset.saturating_add(1),
                ResponseFault::Size => {
                    range.object_size = range
                        .offset
                        .saturating_add(range.bytes.len() as u64)
                        .saturating_sub(1);
                }
                ResponseFault::Provenance => range.provenance.clear(),
                ResponseFault::DifferentProvenance => {
                    range.provenance = alternate_provenance();
                }
            }
        }
        Ok(range)
    }
}

#[test]
fn frames_striped_journal_and_freezes_namespace_before_opaque_mutation() {
    let layout = striped_layout();
    let payloads = vec![
        lid_event(1),
        opaque_event(51, 70 * 1024),
        opaque_event(20, 32),
    ];
    let (mut reader, write_pos, frame_ends) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();

    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();

    assert_eq!(
        replay.framing_status,
        CephFsJournalFramingStatus::CompleteToHeaderTail
    );
    assert_eq!(replay.framing_safe_pos, write_pos);
    assert_eq!(replay.events.len(), 3);
    assert_eq!(replay.namespace_safe_pos, Some(frame_ends[1]));
    assert_eq!(
        replay.namespace_stop_reason,
        Some(CephFsJournalNamespaceStopReason::MutationPayloadUnsupported)
    );
    assert!(
        replay.events[1]
            .spans
            .iter()
            .map(|span| span.locator.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 2
    );
    assert_eq!(replay.replay_sha256.len(), 64);
}

#[test]
fn sequence_conflict_freezes_audit_but_framing_and_provenance_continue() {
    let layout = single_stripe_layout();
    let payloads = vec![lid_event(2), segment_event(2), opaque_event(51, 0)];
    let (mut reader, write_pos, frame_ends) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();

    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();

    assert_eq!(
        replay.framing_status,
        CephFsJournalFramingStatus::CompleteToHeaderTail
    );
    assert_eq!(replay.stop_reason, None);
    assert_eq!(replay.framing_safe_pos, write_pos);
    assert_eq!(replay.sequence_safe_pos, Some(frame_ends[0]));
    assert_eq!(
        replay.sequence_stop_reason.map(|reason| reason.as_str()),
        Some("conflict")
    );
    assert_eq!(
        replay.namespace_stop_reason,
        Some(CephFsJournalNamespaceStopReason::SequenceConflict)
    );
    assert_eq!(replay.events.len(), 3);
    assert_eq!(replay.events[0].sequence_status.as_str(), "validated");
    assert_eq!(replay.events[1].sequence_status.as_str(), "frozen");
    assert_eq!(replay.events[2].sequence_status.as_str(), "frozen");
    assert!(replay.events.iter().all(|event| !event.spans.is_empty()));
}

#[test]
fn boundary_sets_rank_local_event_sequence_and_ordinary_events_increment() {
    let layout = single_stripe_layout();
    let payloads = vec![
        lid_event(1),
        opaque_event(51, 0),
        segment_event(3),
        opaque_event(51, 0),
    ];
    let (mut reader, _, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();

    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();

    assert_eq!(
        replay
            .events
            .iter()
            .map(|event| (
                event.rank_local_segment_sequence,
                event.rank_local_event_sequence
            ))
            .collect::<Vec<_>>(),
        vec![(Some(1), 1), (Some(1), 2), (Some(3), 3), (Some(3), 4)]
    );
}

#[test]
fn non_initial_lid_is_retained_but_ignored_semantically() {
    let layout = single_stripe_layout();
    let payloads = vec![
        lid_event(1),
        opaque_event(51, 0),
        lid_event(999),
        opaque_event(51, 0),
    ];
    let (mut reader, write_pos, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();

    assert_eq!(replay.framing_safe_pos, write_pos);
    assert_eq!(replay.sequence_stop_reason, None);
    assert_eq!(
        replay
            .events
            .iter()
            .map(|event| (
                event.rank_local_segment_sequence,
                event.rank_local_event_sequence,
                event.sequence_status.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some(1), 1, "validated"),
            (Some(1), 2, "validated"),
            (Some(1), 2, "ignored_non_initial_lid"),
            (Some(1), 3, "validated"),
        ]
    );
}

#[test]
fn subtree_and_reset_boundaries_use_encoded_or_logical_sequence() {
    let layout = single_stripe_layout();
    let payloads = vec![subtree_event_v5(), opaque_event(51, 0)];
    let (mut reader, _, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(
        replay.events[0].rank_local_segment_sequence,
        Some(replay.events[0].frame.logical_offset)
    );
    assert_eq!(
        replay.events[1].rank_local_event_sequence,
        replay.events[0].frame.logical_offset + 1
    );

    let payloads = vec![subtree_event_v6(10), reset_event(), opaque_event(51, 0)];
    let (mut reader, _, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(replay.events[0].rank_local_segment_sequence, Some(10));
    assert_eq!(
        replay.events[1].rank_local_segment_sequence,
        Some(replay.events[1].frame.logical_offset)
    );
    assert_eq!(
        replay.events[2].rank_local_event_sequence,
        replay.events[1].frame.logical_offset + 1
    );
}

#[test]
fn subtree_boundary_does_not_claim_decoded_namespace_state() {
    let layout = single_stripe_layout();
    let payloads = vec![subtree_event_v6(10), opaque_event(51, 0)];
    let (mut reader, write_pos, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();

    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();

    assert_eq!(replay.framing_safe_pos, write_pos);
    assert_eq!(replay.namespace_safe_pos, None);
    assert_eq!(
        replay.namespace_stop_reason,
        Some(CephFsJournalNamespaceStopReason::MutationPayloadUnsupported)
    );
}

#[test]
fn truncated_and_unknown_events_have_distinct_safe_boundaries() {
    let layout = single_stripe_layout();
    let period = layout.period().unwrap();
    let descriptor = descriptor();
    let mut reader = FixtureObjectReader::new(descriptor.clone());
    insert_pointer(&mut reader, 0);
    let mut prefix = SENTINEL.to_le_bytes().to_vec();
    prefix.extend_from_slice(&100u32.to_le_bytes());
    reader.insert(
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 1).unwrap(),
        prefix,
    );
    reader.insert(
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 0).unwrap(),
        header(layout, period, period + 20),
    );
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(
        replay.stop_reason,
        Some(CephFsJournalStopReason::TruncatedFrame)
    );
    assert_eq!(replay.framing_safe_pos, period);

    let payloads = vec![lid_event(1), opaque_event(999, 0)];
    let (mut reader, write_pos, frame_ends) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(replay.framing_safe_pos, write_pos);
    assert_eq!(replay.namespace_safe_pos, Some(frame_ends[0]));
    assert_eq!(replay.sequence_safe_pos, Some(frame_ends[0]));
    assert_eq!(
        replay.sequence_stop_reason.map(|reason| reason.as_str()),
        Some("unknown_event")
    );
    assert_eq!(
        replay.namespace_stop_reason,
        Some(CephFsJournalNamespaceStopReason::UnknownEvent)
    );
}

#[test]
fn clean_backed_and_budgeted_journals_remain_explicit() {
    let layout = single_stripe_layout();
    let descriptor = descriptor();
    let period = layout.period().unwrap();
    let mut reader = FixtureObjectReader::new(descriptor.clone());
    insert_pointer(&mut reader, 0);
    reader.insert(
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 0).unwrap(),
        header(layout, period, period),
    );
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(replay.framing_status, CephFsJournalFramingStatus::Clean);
    assert_eq!(replay.namespace_safe_pos, Some(period));

    let payloads = vec![lid_event(1), opaque_event(51, 128)];
    let (mut reader, _, _) = journal_fixture(layout, &payloads, 0x300);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(
        replay.namespace_stop_reason,
        Some(CephFsJournalNamespaceStopReason::BackupJournalPresent)
    );

    let (mut reader, _, _) = journal_fixture(layout, &payloads, 0);
    let descriptor = reader.descriptor.clone();
    let replay = replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits {
            max_bytes: 16,
            ..CephFsJournalReplayLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        replay.stop_reason,
        Some(CephFsJournalStopReason::ByteBudget)
    );
}

#[test]
fn retained_state_span_and_provenance_budgets_fail_closed() {
    let layout = single_stripe_layout();
    for (limits, reason) in [
        (
            CephFsJournalReplayLimits {
                max_source_spans: 2,
                ..CephFsJournalReplayLimits::default()
            },
            CephFsJournalStopReason::SourceSpanBudget,
        ),
        (
            CephFsJournalReplayLimits {
                max_provenance_entries: 2,
                ..CephFsJournalReplayLimits::default()
            },
            CephFsJournalStopReason::ProvenanceBudget,
        ),
        (
            CephFsJournalReplayLimits {
                max_retained_bytes: 2 * 1024,
                ..CephFsJournalReplayLimits::default()
            },
            CephFsJournalStopReason::RetainedMemoryBudget,
        ),
    ] {
        let (mut reader, _, _) = journal_fixture(layout, &[lid_event(1)], 0);
        let descriptor = reader.descriptor.clone();
        let replay = replay_cephfs_journal(&descriptor, RANK, &mut reader, limits).unwrap();
        assert_eq!(replay.stop_reason, Some(reason));
        assert!(replay.events.is_empty());
    }

    let (mut reader, _, _) = journal_fixture(layout, &[], 0);
    let descriptor = reader.descriptor.clone();
    assert!(matches!(
        replay_cephfs_journal(
            &descriptor,
            RANK,
            &mut reader,
            CephFsJournalReplayLimits {
                max_retained_bytes: u64::MAX,
                ..CephFsJournalReplayLimits::default()
            },
        ),
        Err(CephFsJournalReplayError::InvalidLimits)
    ));
}

#[test]
fn control_responses_are_validated_against_request_and_inspect() {
    let layout = single_stripe_layout();
    for fault in [
        ResponseFault::Locator,
        ResponseFault::Size,
        ResponseFault::Provenance,
    ] {
        let (reader, _, _) = journal_fixture(layout, &[], 0);
        let descriptor = reader.descriptor.clone();
        let mut reader = FaultingObjectReader::inspect(reader, 1, fault);
        assert_response_mismatch(&descriptor, &mut reader);
    }
    for fault in [
        ResponseFault::Locator,
        ResponseFault::Offset,
        ResponseFault::Size,
        ResponseFault::Provenance,
        ResponseFault::DifferentProvenance,
    ] {
        let (reader, _, _) = journal_fixture(layout, &[], 0);
        let descriptor = reader.descriptor.clone();
        let mut reader = FaultingObjectReader::range(reader, 1, fault);
        assert_response_mismatch(&descriptor, &mut reader);
    }
}

#[test]
fn stream_range_response_mismatch_stops_without_accepting_frame() {
    let layout = single_stripe_layout();
    for fault in [
        ResponseFault::Locator,
        ResponseFault::Offset,
        ResponseFault::Size,
        ResponseFault::Provenance,
    ] {
        let (reader, _, _) = journal_fixture(layout, &[lid_event(1)], 0);
        let descriptor = reader.descriptor.clone();
        let expected_safe_pos = layout.period().unwrap();
        let mut reader = FaultingObjectReader::range(reader, 3, fault);
        let replay = replay_cephfs_journal(
            &descriptor,
            RANK,
            &mut reader,
            CephFsJournalReplayLimits::default(),
        )
        .unwrap();
        assert_eq!(
            replay.stop_reason,
            Some(CephFsJournalStopReason::ResponseMismatch)
        );
        assert_eq!(replay.framing_safe_pos, expected_safe_pos);
        assert!(replay.events.is_empty());
    }
}

#[test]
fn stale_rank_incarnation_and_cross_pool_header_are_hard_errors() {
    let layout = single_stripe_layout();
    let (mut reader, _, _) = journal_fixture(layout, &[], 0);
    let mut stale = reader.descriptor.clone();
    stale.rank_bindings[0].incarnation += 1;
    assert!(matches!(
        replay_cephfs_journal(
            &stale,
            RANK,
            &mut reader,
            CephFsJournalReplayLimits::default(),
        ),
        Err(CephFsJournalReplayError::InvalidRankBinding { .. })
    ));

    let mut wrong_layout = layout;
    wrong_layout.pool_id = 8;
    let (mut reader, _, _) = journal_fixture(wrong_layout, &[], 0);
    let descriptor = reader.descriptor.clone();
    assert!(matches!(
        replay_cephfs_journal(
            &descriptor,
            RANK,
            &mut reader,
            CephFsJournalReplayLimits::default(),
        ),
        Err(CephFsJournalReplayError::HeaderPoolMismatch)
    ));
}

#[test]
fn pointer_backup_inode_must_belong_to_the_same_rank_and_differ_from_front() {
    let layout = single_stripe_layout();
    for invalid_back in [FRONT_INODE, 0x301, u64::MAX] {
        let (mut reader, _, _) = journal_fixture(layout, &[], invalid_back);
        let descriptor = reader.descriptor.clone();
        assert!(matches!(
            replay_cephfs_journal(
                &descriptor,
                RANK,
                &mut reader,
                CephFsJournalReplayLimits::default(),
            ),
            Err(CephFsJournalReplayError::PointerInodeMismatch)
        ));
    }
}

#[test]
fn discovery_requires_a_pointer_for_each_current_rank_and_ignores_stale_rank_objects() {
    let descriptor = descriptor();
    let pointer = format_cephfs_journal_pointer_object_name(RANK).unwrap();
    let stale = format_cephfs_journal_pointer_object_name(1).unwrap();
    let inventory = merged_inventory(
        &descriptor,
        vec![
            locator(&descriptor, pointer).canonical(),
            locator(&descriptor, stale).canonical(),
        ],
    );
    let candidates = discover_cephfs_journal_ranks(&descriptor, &inventory).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].rank, RANK);
    assert_eq!(candidates[0].incarnation, 4);

    let missing = merged_inventory(&descriptor, Vec::new());
    assert!(matches!(
        discover_cephfs_journal_ranks(&descriptor, &missing),
        Err(CephFsJournalDiscoveryError::MissingPointer { rank: RANK })
    ));

    let mut cross_epoch = inventory;
    cross_epoch.fsmap_epoch += 1;
    assert!(matches!(
        discover_cephfs_journal_ranks(&descriptor, &cross_epoch),
        Err(CephFsJournalDiscoveryError::InvalidInventoryBinding)
    ));

    let inventory = merged_inventory(
        &descriptor,
        vec![locator(
            &descriptor,
            format_cephfs_journal_pointer_object_name(RANK).unwrap(),
        )
        .canonical()],
    );
    let mut stale = descriptor.clone();
    stale.rank_bindings[0].incarnation += 1;
    assert!(matches!(
        discover_cephfs_journal_ranks(&stale, &inventory),
        Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: RANK })
    ));

    let mut inactive = descriptor;
    inactive.daemons[0].state = CephMdsState::Standby;
    assert!(matches!(
        discover_cephfs_journal_ranks(&inactive, &inventory),
        Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: RANK })
    ));
}

fn descriptor() -> CephFsDescriptor {
    CephFsDescriptor {
        identity: "ceph-fs:cluster-a:1:17:7".to_string(),
        cluster_identity: "cluster-a".to_string(),
        filesystem_id: 1,
        name: "cephfs-a".to_string(),
        fsmap_epoch: FSMAP_EPOCH,
        mdsmap_epoch: 19,
        state: CephFsDescriptorState::Present,
        metadata_pool: CephFsPoolBinding {
            pool_id: POOL_ID,
            role: CephFsPoolRole::Metadata,
            provenance: Vec::new(),
        },
        data_pools: Vec::new(),
        rank_bindings: vec![CephFsRankBinding {
            rank: RANK,
            gid: 123,
            incarnation: 4,
        }],
        daemons: vec![CephMdsDaemon {
            gid: 123,
            name: "mds-a".to_string(),
            rank: RANK as i32,
            incarnation: 4,
            state: CephMdsState::Active,
            state_sequence: 99,
        }],
        provenance: Vec::new(),
    }
}

fn journal_fixture(
    layout: CephFsJournalLayout,
    payloads: &[Vec<u8>],
    back: u64,
) -> (FixtureObjectReader, u64, Vec<u64>) {
    let descriptor = descriptor();
    let mut reader = FixtureObjectReader::new(descriptor);
    insert_pointer(&mut reader, back);
    let period = layout.period().unwrap();
    let mut position = period;
    let mut stream = Vec::new();
    let mut frame_ends = Vec::new();
    for payload in payloads {
        let frame = resilient_frame(payload, position);
        position += frame.len() as u64;
        frame_ends.push(position);
        stream.extend_from_slice(&frame);
    }
    reader.insert(
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 0).unwrap(),
        header(layout, period, position),
    );
    store_logical_stream(&mut reader, layout, period, &stream);
    (reader, position, frame_ends)
}

fn insert_pointer(reader: &mut FixtureObjectReader, back: u64) {
    let mut payload = FRONT_INODE.to_le_bytes().to_vec();
    payload.extend_from_slice(&back.to_le_bytes());
    reader.insert(
        format_cephfs_journal_pointer_object_name(RANK).unwrap(),
        envelope(1, 1, &payload),
    );
}

fn store_logical_stream(
    reader: &mut FixtureObjectReader,
    layout: CephFsJournalLayout,
    start: u64,
    bytes: &[u8],
) {
    for extent in plan_cephfs_journal_range(layout, start, bytes.len()).unwrap() {
        let name =
            format_cephfs_journal_data_object_name(RANK, FRONT_INODE, extent.object_index).unwrap();
        let locator = locator(&reader.descriptor, name.clone()).canonical();
        let object = reader.objects.entry(locator).or_default();
        let end = extent.object_offset as usize + extent.length;
        object.resize(object.len().max(end), 0);
        let source_offset = (extent.logical_offset - start) as usize;
        object[extent.object_offset as usize..end]
            .copy_from_slice(&bytes[source_offset..source_offset + extent.length]);
    }
}

fn header(layout: CephFsJournalLayout, expire_pos: u64, write_pos: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    append_string(&mut payload, CEPHFS_JOURNAL_MAGIC);
    payload.extend_from_slice(&layout.period().unwrap().to_le_bytes());
    payload.extend_from_slice(&expire_pos.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&write_pos.to_le_bytes());
    for value in [
        layout.stripe_unit,
        layout.stripe_count,
        layout.object_size,
        0,
        0,
        0,
        layout.pool_id as u32,
    ] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.push(1);
    envelope(2, 2, &payload)
}

fn lid_event(sequence: u64) -> Vec<u8> {
    boundary_event(101, sequence)
}

fn segment_event(sequence: u64) -> Vec<u8> {
    boundary_event(100, sequence)
}

fn subtree_event_v5() -> Vec<u8> {
    versioned_event(2, &envelope(5, 5, b"opaque-v5-fields"))
}

fn subtree_event_v6(sequence: u64) -> Vec<u8> {
    let mut payload = b"opaque-v6-fields".to_vec();
    payload.extend_from_slice(&sequence.to_le_bytes());
    versioned_event(2, &envelope(6, 5, &payload))
}

fn reset_event() -> Vec<u8> {
    versioned_event(9, &envelope(2, 2, &[0; 8]))
}

fn boundary_event(event_type: u32, sequence: u64) -> Vec<u8> {
    let nested = envelope(1, 1, &sequence.to_le_bytes());
    versioned_event(event_type, &nested)
}

fn opaque_event(event_type: u32, payload_length: usize) -> Vec<u8> {
    versioned_event(event_type, &vec![0x5a; payload_length])
}

fn versioned_event(event_type: u32, event_payload: &[u8]) -> Vec<u8> {
    let mut payload = event_type.to_le_bytes().to_vec();
    payload.extend_from_slice(event_payload);
    let mut event = 0u32.to_le_bytes().to_vec();
    event.extend_from_slice(&envelope(1, 1, &payload));
    event
}

fn resilient_frame(payload: &[u8], start: u64) -> Vec<u8> {
    let mut bytes = SENTINEL.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&start.to_le_bytes());
    bytes
}

fn envelope(version: u8, compat: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![version, compat];
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn locator(descriptor: &CephFsDescriptor, object_name: String) -> CephFsObjectLocator {
    CephFsObjectLocator::new(
        descriptor.filesystem_id,
        descriptor.metadata_pool.pool_id,
        Vec::new(),
        object_name.into_bytes(),
        descriptor.fsmap_epoch,
    )
    .unwrap()
}

fn provenance() -> Vec<CephFsObjectReadProvenance> {
    vec![CephFsObjectReadProvenance {
        data_source_id: "source-a".to_string(),
        inventory_id: "inventory-a".to_string(),
        object_identity_sha256: "a".repeat(64),
    }]
}

fn alternate_provenance() -> Vec<CephFsObjectReadProvenance> {
    vec![CephFsObjectReadProvenance {
        data_source_id: "source-a".to_string(),
        inventory_id: "inventory-a".to_string(),
        object_identity_sha256: "b".repeat(64),
    }]
}

fn assert_response_mismatch<R: CephFsObjectRangeReader>(
    descriptor: &CephFsDescriptor,
    reader: &mut R,
) {
    assert!(matches!(
        replay_cephfs_journal(
            descriptor,
            RANK,
            reader,
            CephFsJournalReplayLimits::default(),
        ),
        Err(CephFsJournalReplayError::Object(
            CephFsObjectReadError::ResponseMismatch { .. }
        ))
    ));
}

fn merged_inventory(
    descriptor: &CephFsDescriptor,
    locators: Vec<String>,
) -> CephFsMergedMetadataInventory {
    CephFsMergedMetadataInventory {
        filesystem_identity: descriptor.identity.clone(),
        filesystem_id: descriptor.filesystem_id,
        fsmap_epoch: descriptor.fsmap_epoch,
        metadata_pool_id: descriptor.metadata_pool.pool_id,
        object_count: locators.len() as u64,
        unknown_object_count: 0,
        inventory_sha256: "b".repeat(64),
        objects: locators
            .into_iter()
            .map(|locator| CephFsMergedMetadataObject {
                locator,
                candidate_mask: 0,
                classification_state: "classified".to_string(),
                classifier_rule: "journal_pointer".to_string(),
                record_sha256: "c".repeat(64),
                provenance: Vec::new(),
            })
            .collect(),
    }
}

fn single_stripe_layout() -> CephFsJournalLayout {
    CephFsJournalLayout {
        stripe_unit: 64 * 1024,
        stripe_count: 1,
        object_size: 64 * 1024,
        pool_id: POOL_ID,
    }
}

fn striped_layout() -> CephFsJournalLayout {
    CephFsJournalLayout {
        stripe_unit: 64 * 1024,
        stripe_count: 2,
        object_size: 64 * 1024,
        pool_id: POOL_ID,
    }
}
