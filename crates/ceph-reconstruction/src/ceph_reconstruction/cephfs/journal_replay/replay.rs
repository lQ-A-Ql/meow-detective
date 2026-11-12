use sha2::{Digest, Sha256};

use ceph_wire::{
    decode_cephfs_journal_frame, decode_cephfs_journal_frame_prefix, decode_cephfs_journal_header,
    decode_cephfs_journal_pointer, format_cephfs_journal_data_object_name,
    format_cephfs_journal_pointer_object_name, CephFsJournalEventEncoding, CephFsJournalEventKind,
    CephFsJournalStreamFormat,
};

use super::super::{
    validate_metadata_response, validate_range_response, CephFsDescriptor, CephFsObjectLocator,
    CephFsObjectRangeReader, CephFsObjectReadError,
};
use super::{
    binding::validate_rank_binding,
    digest,
    stream::{JournalStreamError, JournalStreamReader},
    types::{
        control_object_limit, CephFsJournalSequenceStatus, CephFsJournalSequenceStopReason,
        RetainedBudget,
    },
    CephFsJournalFramingStatus, CephFsJournalNamespaceStopReason, CephFsJournalReplay,
    CephFsJournalReplayError, CephFsJournalReplayEvent, CephFsJournalReplayLimits,
    CephFsJournalSourceSpan, CephFsJournalStopReason,
};

struct FrameScan {
    safe_pos: u64,
    status: CephFsJournalFramingStatus,
    stop_reason: Option<CephFsJournalStopReason>,
    namespace_safe_pos: Option<u64>,
    namespace_stop_reason: Option<CephFsJournalNamespaceStopReason>,
    sequence_safe_pos: Option<u64>,
    sequence_stop_reason: Option<CephFsJournalSequenceStopReason>,
    events: Vec<CephFsJournalReplayEvent>,
}

struct ControlSpans<'a> {
    pointer: &'a [CephFsJournalSourceSpan],
    header: &'a [CephFsJournalSourceSpan],
}

struct SequenceState {
    current_segment: Option<u64>,
    event_sequence: u64,
    event_seen: bool,
    safe_pos: Option<u64>,
    stop_reason: Option<CephFsJournalSequenceStopReason>,
}

#[derive(Clone, Copy)]
struct SequenceObservation {
    segment_sequence: Option<u64>,
    event_sequence: u64,
    status: CephFsJournalSequenceStatus,
}

struct NamespaceState {
    major_seen: bool,
    safe_pos: Option<u64>,
    stop_reason: Option<CephFsJournalNamespaceStopReason>,
}

pub fn replay_cephfs_journal<R: CephFsObjectRangeReader>(
    descriptor: &CephFsDescriptor,
    rank: u32,
    reader: &mut R,
    limits: CephFsJournalReplayLimits,
) -> Result<CephFsJournalReplay, CephFsJournalReplayError> {
    let limits = limits.validate()?;
    let rank_binding = validate_rank_binding(descriptor, rank)?;
    let pointer_locator = control_locator(
        descriptor,
        format_cephfs_journal_pointer_object_name(rank)
            .ok_or(CephFsJournalReplayError::InvalidRankBinding { rank })?,
    )?;
    let (pointer_bytes, pointer_spans) = read_control_object(descriptor, reader, &pointer_locator)?;
    let pointer = decode_cephfs_journal_pointer(&pointer_bytes)?;
    let header_name = format_cephfs_journal_data_object_name(rank, pointer.front, 0)
        .ok_or(CephFsJournalReplayError::PointerInodeMismatch)?;
    if pointer.back != 0
        && (pointer.back == pointer.front
            || format_cephfs_journal_data_object_name(rank, pointer.back, 0).is_none())
    {
        return Err(CephFsJournalReplayError::PointerInodeMismatch);
    }
    let header_locator = control_locator(descriptor, header_name)?;
    let (header_bytes, header_spans) = read_control_object(descriptor, reader, &header_locator)?;
    let header = decode_cephfs_journal_header(&header_bytes)?;
    if header.layout.pool_id != descriptor.metadata_pool.pool_id {
        return Err(CephFsJournalReplayError::HeaderPoolMismatch);
    }
    validate_control_budget(&pointer_spans, &header_spans, limits)?;
    let scan = scan_frames(
        descriptor,
        rank,
        pointer,
        &header,
        reader,
        limits,
        ControlSpans {
            pointer: &pointer_spans,
            header: &header_spans,
        },
    );
    let mut replay = CephFsJournalReplay {
        filesystem_identity: descriptor.identity.clone(),
        fsmap_epoch: descriptor.fsmap_epoch,
        mdsmap_epoch: descriptor.mdsmap_epoch,
        rank: rank_binding.rank,
        rank_gid: rank_binding.gid,
        rank_incarnation: rank_binding.incarnation,
        pointer,
        committed_header_tail: header.write_pos,
        header,
        framing_safe_pos: scan.safe_pos,
        namespace_safe_pos: scan.namespace_safe_pos,
        sequence_safe_pos: scan.sequence_safe_pos,
        framing_status: scan.status,
        stop_reason: scan.stop_reason,
        namespace_stop_reason: scan.namespace_stop_reason,
        sequence_stop_reason: scan.sequence_stop_reason,
        pointer_spans,
        header_spans,
        events: scan.events,
        replay_sha256: String::new(),
    };
    replay.replay_sha256 = digest::replay_sha256(&replay);
    Ok(replay)
}

fn scan_frames<R: CephFsObjectRangeReader>(
    descriptor: &CephFsDescriptor,
    rank: u32,
    pointer: ceph_wire::CephFsJournalPointer,
    header: &ceph_wire::CephFsJournalHeader,
    reader: &mut R,
    limits: CephFsJournalReplayLimits,
    control_spans: ControlSpans<'_>,
) -> FrameScan {
    let mut namespace = NamespaceState::new(pointer.back != 0);
    if header.expire_pos == header.write_pos {
        if pointer.back == 0 {
            namespace.safe_pos = Some(header.write_pos);
        }
        return FrameScan {
            safe_pos: header.write_pos,
            status: CephFsJournalFramingStatus::Clean,
            stop_reason: None,
            namespace_safe_pos: namespace.safe_pos,
            namespace_stop_reason: namespace.stop_reason,
            sequence_safe_pos: Some(header.write_pos),
            sequence_stop_reason: None,
            events: Vec::new(),
        };
    }
    let mut stream =
        JournalStreamReader::new(descriptor, rank, pointer.front, header.layout, reader);
    let mut sequence = SequenceState::new(header.expire_pos);
    let mut retained = RetainedBudget::new(control_spans.pointer, control_spans.header);
    let mut events = Vec::new();
    let mut position = header.expire_pos;
    let mut stop_reason = None;
    while position < header.write_pos {
        match read_next_frame(&mut stream, header, limits, position, events.len()) {
            Ok((frame, spans)) => {
                if let Err(reason) = retained.admit(&frame, &spans, limits) {
                    stop_reason = Some(reason);
                    break;
                }
                position = frame.logical_end;
                let observation = sequence.observe(&frame);
                namespace.observe(&frame, observation, sequence.stop_reason);
                events.push(CephFsJournalReplayEvent {
                    ordinal: events.len() as u64,
                    rank_local_segment_sequence: observation.segment_sequence,
                    rank_local_event_sequence: observation.event_sequence,
                    sequence_status: observation.status,
                    frame,
                    spans,
                });
            }
            Err(reason) => {
                stop_reason = Some(reason);
                break;
            }
        }
    }
    let status = if stop_reason.is_some() {
        namespace.mark_framing_incomplete();
        CephFsJournalFramingStatus::Incomplete
    } else {
        namespace.finish();
        CephFsJournalFramingStatus::CompleteToHeaderTail
    };
    FrameScan {
        safe_pos: position,
        status,
        stop_reason,
        namespace_safe_pos: namespace.safe_pos,
        namespace_stop_reason: namespace.stop_reason,
        sequence_safe_pos: sequence.safe_pos,
        sequence_stop_reason: sequence.stop_reason,
        events,
    }
}

fn read_next_frame<R: CephFsObjectRangeReader>(
    stream: &mut JournalStreamReader<'_, R>,
    header: &ceph_wire::CephFsJournalHeader,
    limits: CephFsJournalReplayLimits,
    position: u64,
    event_count: usize,
) -> Result<(ceph_wire::CephFsJournalFrame, Vec<CephFsJournalSourceSpan>), CephFsJournalStopReason>
{
    if event_count >= limits.max_events {
        return Err(CephFsJournalStopReason::EventBudget);
    }
    let prefix_length = match header.stream_format {
        CephFsJournalStreamFormat::Legacy => 4usize,
        CephFsJournalStreamFormat::Resilient => 12usize,
    };
    let remaining = header.write_pos - position;
    let consumed = position - header.expire_pos;
    if remaining < prefix_length as u64 {
        return Err(CephFsJournalStopReason::TruncatedFrame);
    }
    if consumed.saturating_add(prefix_length as u64) > limits.max_bytes {
        return Err(CephFsJournalStopReason::ByteBudget);
    }
    let prefix = stream
        .read_exact(position, prefix_length)
        .map_err(stream_stop_reason)?;
    let decoded_prefix = decode_cephfs_journal_frame_prefix(
        &prefix.bytes,
        position,
        header.stream_format,
        limits.max_event_bytes,
    )
    .map_err(|_| CephFsJournalStopReason::InvalidFrame)?;
    if decoded_prefix.total_length as u64 > remaining {
        return Err(CephFsJournalStopReason::TruncatedFrame);
    }
    if consumed.saturating_add(decoded_prefix.total_length as u64) > limits.max_bytes {
        return Err(CephFsJournalStopReason::ByteBudget);
    }
    let tail_length = decoded_prefix.total_length - prefix_length;
    let tail = stream
        .read_exact(position + prefix_length as u64, tail_length)
        .map_err(stream_stop_reason)?;
    let mut bytes = prefix.bytes;
    bytes.extend_from_slice(&tail.bytes);
    let mut spans = prefix.spans;
    spans.extend(tail.spans);
    let frame = decode_cephfs_journal_frame(
        &bytes,
        position,
        header.stream_format,
        limits.max_event_bytes,
    )
    .map_err(|_| CephFsJournalStopReason::InvalidFrame)?;
    Ok((frame, spans))
}

fn control_locator(
    descriptor: &CephFsDescriptor,
    object_name: String,
) -> Result<CephFsObjectLocator, CephFsJournalReplayError> {
    Ok(CephFsObjectLocator::new(
        descriptor.filesystem_id,
        descriptor.metadata_pool.pool_id,
        Vec::new(),
        object_name.into_bytes(),
        descriptor.fsmap_epoch,
    )?)
}

fn read_control_object<R: CephFsObjectRangeReader>(
    descriptor: &CephFsDescriptor,
    reader: &mut R,
    locator: &CephFsObjectLocator,
) -> Result<(Vec<u8>, Vec<CephFsJournalSourceSpan>), CephFsJournalReplayError> {
    let metadata = reader.inspect_object(locator)?;
    validate_metadata_response(descriptor, locator, &metadata)?;
    if metadata.object_size == 0 || metadata.object_size > control_object_limit() {
        return Err(CephFsJournalReplayError::ControlObjectTooLarge);
    }
    let length = usize::try_from(metadata.object_size)
        .map_err(|_| CephFsJournalReplayError::ControlObjectTooLarge)?;
    let range = reader.read_range(locator, 0, length)?;
    validate_range_response(descriptor, locator, 0, length, Some(&metadata), &range)?;
    let span = CephFsJournalSourceSpan {
        locator: range.locator,
        logical_offset: 0,
        object_offset: 0,
        length: metadata.object_size,
        range_sha256: format!("{:x}", Sha256::digest(&range.bytes)),
        provenance: range.provenance,
    };
    Ok((range.bytes, vec![span]))
}

fn stream_stop_reason(error: JournalStreamError) -> CephFsJournalStopReason {
    match error {
        JournalStreamError::Object(
            CephFsObjectReadError::MetadataConflict { .. }
            | CephFsObjectReadError::ReplicaCoverageIncomplete { .. }
            | CephFsObjectReadError::ByteConflict { .. },
        ) => CephFsJournalStopReason::ReplicaConflict,
        JournalStreamError::Object(CephFsObjectReadError::ResponseMismatch { .. }) => {
            CephFsJournalStopReason::ResponseMismatch
        }
        JournalStreamError::Object(_) => CephFsJournalStopReason::ObjectUnavailable,
        JournalStreamError::InvalidMapping
        | JournalStreamError::Inventory(_)
        | JournalStreamError::Wire(_) => CephFsJournalStopReason::InvalidFrame,
    }
}

impl SequenceState {
    fn new(start: u64) -> Self {
        Self {
            current_segment: None,
            event_sequence: 0,
            event_seen: false,
            safe_pos: Some(start),
            stop_reason: None,
        }
    }

    fn observe(&mut self, frame: &ceph_wire::CephFsJournalFrame) -> SequenceObservation {
        if self.stop_reason.is_some() {
            return self.observation(CephFsJournalSequenceStatus::Frozen);
        }
        if !frame.event.semantic_state.is_supported() {
            self.freeze(if frame.event.semantic_state.is_unknown() {
                CephFsJournalSequenceStopReason::UnknownEvent
            } else {
                CephFsJournalSequenceStopReason::UnsupportedSemantics
            });
            return self.observation(CephFsJournalSequenceStatus::Frozen);
        }
        if frame.event.kind == CephFsJournalEventKind::Lid && self.current_segment.is_some() {
            self.safe_pos = Some(frame.logical_end);
            return self.observation(CephFsJournalSequenceStatus::IgnoredNonInitialLid);
        }
        if frame.event.kind.is_boundary() {
            let Some(sequence) = frame.event.boundary_sequence.resolve(frame.logical_offset) else {
                self.freeze(CephFsJournalSequenceStopReason::UnsupportedSemantics);
                return self.observation(CephFsJournalSequenceStatus::Frozen);
            };
            if self.event_seen && sequence <= self.event_sequence {
                self.freeze(CephFsJournalSequenceStopReason::Conflict);
                return self.observation(CephFsJournalSequenceStatus::Frozen);
            }
            self.current_segment = Some(sequence);
            self.event_sequence = sequence;
        } else if let Some(next) = self.event_sequence.checked_add(1) {
            self.event_sequence = next;
        } else {
            self.freeze(CephFsJournalSequenceStopReason::Overflow);
            return self.observation(CephFsJournalSequenceStatus::Frozen);
        }
        self.event_seen = true;
        self.safe_pos = Some(frame.logical_end);
        self.observation(CephFsJournalSequenceStatus::Validated)
    }

    fn freeze(&mut self, reason: CephFsJournalSequenceStopReason) {
        self.stop_reason = Some(reason);
    }

    fn observation(&self, status: CephFsJournalSequenceStatus) -> SequenceObservation {
        SequenceObservation {
            segment_sequence: self.current_segment,
            event_sequence: self.event_sequence,
            status,
        }
    }
}

impl NamespaceState {
    fn new(backup_present: bool) -> Self {
        Self {
            major_seen: false,
            safe_pos: None,
            stop_reason: backup_present
                .then_some(CephFsJournalNamespaceStopReason::BackupJournalPresent),
        }
    }

    fn observe(
        &mut self,
        frame: &ceph_wire::CephFsJournalFrame,
        sequence: SequenceObservation,
        sequence_stop_reason: Option<CephFsJournalSequenceStopReason>,
    ) {
        if self.stop_reason.is_some() {
            return;
        }
        if sequence.status == CephFsJournalSequenceStatus::Frozen {
            self.stop_reason = Some(match sequence_stop_reason {
                Some(CephFsJournalSequenceStopReason::Conflict) => {
                    CephFsJournalNamespaceStopReason::SequenceConflict
                }
                Some(CephFsJournalSequenceStopReason::UnknownEvent) => {
                    CephFsJournalNamespaceStopReason::UnknownEvent
                }
                _ => CephFsJournalNamespaceStopReason::SequenceSemanticsUnsupported,
            });
            return;
        }
        if sequence.status == CephFsJournalSequenceStatus::IgnoredNonInitialLid {
            if self.major_seen {
                self.safe_pos = Some(frame.logical_end);
            }
            return;
        }
        if frame.event.encoding == CephFsJournalEventEncoding::Legacy {
            self.stop_reason = Some(CephFsJournalNamespaceStopReason::LegacyEventEncoding);
            return;
        }
        match frame.event.kind {
            CephFsJournalEventKind::SubtreeMap => {
                self.stop_reason =
                    Some(CephFsJournalNamespaceStopReason::MutationPayloadUnsupported);
            }
            CephFsJournalEventKind::ResetJournal | CephFsJournalEventKind::Lid => {
                self.major_seen = true;
                self.safe_pos = Some(frame.logical_end);
            }
            CephFsJournalEventKind::Segment | CephFsJournalEventKind::Noop if self.major_seen => {
                self.safe_pos = Some(frame.logical_end);
            }
            CephFsJournalEventKind::Segment | CephFsJournalEventKind::Noop => {}
            CephFsJournalEventKind::Unknown => {
                self.stop_reason = Some(CephFsJournalNamespaceStopReason::UnknownEvent);
            }
            _ => {
                self.stop_reason =
                    Some(CephFsJournalNamespaceStopReason::MutationPayloadUnsupported);
            }
        }
    }

    fn mark_framing_incomplete(&mut self) {
        if self.stop_reason.is_none() {
            self.stop_reason = Some(CephFsJournalNamespaceStopReason::FramingIncomplete);
        }
    }

    fn finish(&mut self) {
        if self.stop_reason.is_none() && !self.major_seen {
            self.stop_reason = Some(CephFsJournalNamespaceStopReason::NoMajorBoundary);
            self.safe_pos = None;
        }
    }
}

fn validate_control_budget(
    pointer_spans: &[CephFsJournalSourceSpan],
    header_spans: &[CephFsJournalSourceSpan],
    limits: CephFsJournalReplayLimits,
) -> Result<(), CephFsJournalReplayError> {
    if RetainedBudget::new(pointer_spans, header_spans).within(limits) {
        Ok(())
    } else {
        Err(CephFsJournalReplayError::RetainedStateBudget)
    }
}
