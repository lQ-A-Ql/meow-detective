use ceph_wire::{
    decode_cephfs_journal_frame, decode_cephfs_journal_frame_prefix, decode_cephfs_journal_header,
    decode_cephfs_journal_pointer, CephFsJournalEventEncoding, CephFsJournalEventKind,
    CephFsJournalStreamFormat, CephWireError, CEPHFS_JOURNAL_MAGIC, CEPHFS_JOURNAL_MAX_EVENT_BYTES,
};

const FRAME_OFFSET: u64 = 0x40_0000;
const SENTINEL: u64 = 0x3141_5926_5358_9793;

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

fn append_header_payload(bytes: &mut Vec<u8>, include_stream_format: bool) {
    append_string(bytes, CEPHFS_JOURNAL_MAGIC);
    bytes.extend_from_slice(&0x40_0000u64.to_le_bytes());
    bytes.extend_from_slice(&0x40_0010u64.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0x40_0100u64.to_le_bytes());
    for value in [0x10_0000u32, 1, 0x40_0000, 0, 0, 0, 7] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    if include_stream_format {
        bytes.push(1);
    }
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

#[test]
fn pointer_and_header_decode_pinned_ceph_wire_shapes() {
    let mut pointer_payload = 0x200u64.to_le_bytes().to_vec();
    pointer_payload.extend_from_slice(&0x300u64.to_le_bytes());
    assert_eq!(
        decode_cephfs_journal_pointer(&envelope(1, 1, &pointer_payload)).unwrap(),
        ceph_wire::CephFsJournalPointer {
            front: 0x200,
            back: 0x300,
        }
    );

    let mut header_payload = Vec::new();
    append_header_payload(&mut header_payload, true);
    let header = decode_cephfs_journal_header(&envelope(2, 2, &header_payload)).unwrap();
    assert_eq!(header.magic, CEPHFS_JOURNAL_MAGIC);
    assert_eq!(header.unused_pos, 0);
    assert_eq!(header.layout.period().unwrap(), 0x40_0000);
    assert_eq!(header.stream_format, CephFsJournalStreamFormat::Resilient);
}

#[test]
fn compatible_pointer_and_header_envelopes_skip_only_payload_tails() {
    let mut pointer_payload = 0x200u64.to_le_bytes().to_vec();
    pointer_payload.extend_from_slice(&0x300u64.to_le_bytes());
    pointer_payload.extend_from_slice(b"future-pointer-tail");
    let pointer = decode_cephfs_journal_pointer(&envelope(2, 1, &pointer_payload)).unwrap();
    assert_eq!(pointer.front, 0x200);
    assert_eq!(pointer.back, 0x300);

    let mut header_payload = Vec::new();
    append_header_payload(&mut header_payload, true);
    header_payload.extend_from_slice(b"future-header-tail");
    let header = decode_cephfs_journal_header(&envelope(3, 2, &header_payload)).unwrap();
    assert_eq!(header.write_pos, 0x40_0100);
    assert_eq!(header.stream_format, CephFsJournalStreamFormat::Resilient);

    let mut trailing = envelope(2, 1, &pointer_payload);
    trailing.push(0xff);
    assert!(decode_cephfs_journal_pointer(&trailing).is_err());
}

#[test]
fn incompatible_control_envelopes_are_rejected() {
    let mut pointer_payload = 0x200u64.to_le_bytes().to_vec();
    pointer_payload.extend_from_slice(&0u64.to_le_bytes());
    assert!(decode_cephfs_journal_pointer(&envelope(2, 2, &pointer_payload)).is_err());

    let mut header_payload = Vec::new();
    append_header_payload(&mut header_payload, true);
    assert!(decode_cephfs_journal_header(&envelope(3, 3, &header_payload)).is_err());
}

#[test]
fn legacy_header_defaults_to_legacy_stream_format() {
    let mut bytes = vec![1];
    append_header_payload(&mut bytes, false);
    let header = decode_cephfs_journal_header(&bytes).unwrap();
    assert_eq!(header.stream_format, CephFsJournalStreamFormat::Legacy);
}

#[test]
fn resilient_frame_preserves_rank_local_segment_sequence() {
    let segment_payload = envelope(1, 1, &9u64.to_le_bytes());
    let payload = versioned_event(100, &segment_payload);
    let bytes = resilient_frame(&payload, FRAME_OFFSET);
    let prefix = decode_cephfs_journal_frame_prefix(
        &bytes,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(prefix.total_length, payload.len() + 20);

    let frame = decode_cephfs_journal_frame(
        &bytes,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(frame.event.kind, CephFsJournalEventKind::Segment);
    assert_eq!(frame.event.segment_sequence, Some(9));
    assert_eq!(frame.event.boundary_sequence.as_str(), "encoded");
    assert!(matches!(
        frame.event.encoding,
        CephFsJournalEventEncoding::Versioned { .. }
    ));
    assert_eq!(frame.payload_sha256.len(), 64);
}

#[test]
fn boundary_envelopes_follow_pinned_segment_sequence_semantics() {
    let segment = nested_versioned_event(100, 2, 1, &boundary_payload(7, b"tail"), b"outer");
    let frame = decode_resilient(&segment);
    assert_eq!(frame.event.semantic_state.as_str(), "supported");
    assert_eq!(frame.event.boundary_sequence.as_str(), "encoded");
    assert_eq!(frame.event.segment_sequence, Some(7));

    let subtree_v5 = nested_versioned_event(2, 5, 5, b"opaque-v5-fields", b"");
    let frame = decode_resilient(&subtree_v5);
    assert_eq!(frame.event.boundary_sequence.as_str(), "logical_offset");

    let mut subtree_v6_payload = b"opaque-v6-fields".to_vec();
    subtree_v6_payload.extend_from_slice(&19u64.to_le_bytes());
    let subtree_v6 = nested_versioned_event(2, 6, 5, &subtree_v6_payload, b"");
    let frame = decode_resilient(&subtree_v6);
    assert_eq!(frame.event.segment_sequence, Some(19));
    assert_eq!(frame.event.boundary_sequence.as_str(), "encoded");

    let reset = nested_versioned_event(9, 3, 2, b"compatible-reset-tail", b"");
    let frame = decode_resilient(&reset);
    assert_eq!(frame.event.boundary_sequence.as_str(), "logical_offset");
}

#[test]
fn unsafe_future_subtree_and_incompatible_boundaries_remain_framed() {
    let subtree = nested_versioned_event(2, 7, 5, b"opaque-future-fields", b"");
    let frame = decode_resilient(&subtree);
    assert_eq!(frame.logical_offset, FRAME_OFFSET);
    assert_eq!(frame.event.kind, CephFsJournalEventKind::SubtreeMap);
    assert_eq!(frame.event.semantic_state.as_str(), "unsupported_envelope");
    assert_eq!(frame.event.boundary_sequence.as_str(), "unavailable");

    let segment = nested_versioned_event(100, 2, 2, &8u64.to_le_bytes(), b"");
    let frame = decode_resilient(&segment);
    assert_eq!(frame.event.semantic_state.as_str(), "unsupported_envelope");
    assert_eq!(frame.payload_sha256.len(), 64);
}

#[test]
fn malformed_event_semantics_do_not_invalidate_physical_frame() {
    let bytes = resilient_frame(&[0, 0, 0], FRAME_OFFSET);
    let frame = decode_cephfs_journal_frame(
        &bytes,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(frame.event.semantic_state.as_str(), "malformed_envelope");

    let empty = resilient_frame(&[], FRAME_OFFSET);
    let frame = decode_cephfs_journal_frame(
        &empty,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(frame.payload_length, 0);
    assert_eq!(frame.event.semantic_state.as_str(), "malformed_envelope");
}

#[test]
fn classic_and_unknown_events_are_explicit_not_fabricated() {
    let mut classic = 20u32.to_le_bytes().to_vec();
    classic.extend_from_slice(b"opaque");
    let mut legacy_frame = (classic.len() as u32).to_le_bytes().to_vec();
    legacy_frame.extend_from_slice(&classic);
    let decoded = decode_cephfs_journal_frame(
        &legacy_frame,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Legacy,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(decoded.event.encoding, CephFsJournalEventEncoding::Legacy);
    assert_eq!(decoded.event.kind, CephFsJournalEventKind::Update);

    let payload = versioned_event(999, b"opaque");
    let frame = resilient_frame(&payload, FRAME_OFFSET);
    let decoded = decode_cephfs_journal_frame(
        &frame,
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap();
    assert_eq!(decoded.event.kind, CephFsJournalEventKind::Unknown);
}

#[test]
fn corrupt_or_unbounded_frames_fail_closed() {
    let payload = versioned_event(51, b"");
    let mut bad_start = resilient_frame(&payload, FRAME_OFFSET + 1);
    assert!(matches!(
        decode_cephfs_journal_frame(
            &bad_start,
            FRAME_OFFSET,
            CephFsJournalStreamFormat::Resilient,
            CEPHFS_JOURNAL_MAX_EVENT_BYTES,
        ),
        Err(CephWireError::InvalidCephFsJournalFrame { .. })
    ));

    bad_start.truncate(bad_start.len() - 1);
    assert!(matches!(
        decode_cephfs_journal_frame(
            &bad_start,
            FRAME_OFFSET,
            CephFsJournalStreamFormat::Resilient,
            CEPHFS_JOURNAL_MAX_EVENT_BYTES,
        ),
        Err(CephWireError::InvalidCephFsJournalFrame { .. })
    ));

    let mut oversized = SENTINEL.to_le_bytes().to_vec();
    oversized.extend_from_slice(&((CEPHFS_JOURNAL_MAX_EVENT_BYTES as u32) + 1).to_le_bytes());
    assert!(matches!(
        decode_cephfs_journal_frame_prefix(
            &oversized,
            FRAME_OFFSET,
            CephFsJournalStreamFormat::Resilient,
            CEPHFS_JOURNAL_MAX_EVENT_BYTES,
        ),
        Err(CephWireError::CephFsJournalEventTooLarge { .. })
    ));
}

#[test]
fn every_truncated_control_and_frame_prefix_is_rejected() {
    let mut pointer_payload = 0x200u64.to_le_bytes().to_vec();
    pointer_payload.extend_from_slice(&0u64.to_le_bytes());
    let pointer = envelope(1, 1, &pointer_payload);
    for length in 0..pointer.len() {
        assert!(decode_cephfs_journal_pointer(&pointer[..length]).is_err());
    }

    let mut header_payload = Vec::new();
    append_header_payload(&mut header_payload, true);
    let header = envelope(2, 2, &header_payload);
    for length in 0..header.len() {
        assert!(decode_cephfs_journal_header(&header[..length]).is_err());
    }

    let payload = versioned_event(51, b"");
    let frame = resilient_frame(&payload, FRAME_OFFSET);
    for length in 0..frame.len() {
        assert!(decode_cephfs_journal_frame(
            &frame[..length],
            FRAME_OFFSET,
            CephFsJournalStreamFormat::Resilient,
            CEPHFS_JOURNAL_MAX_EVENT_BYTES,
        )
        .is_err());
    }
}

#[test]
fn invalid_header_magic_and_position_order_are_rejected() {
    let mut payload = Vec::new();
    append_header_payload(&mut payload, true);
    payload[4] = b'X';
    assert!(matches!(
        decode_cephfs_journal_header(&envelope(2, 2, &payload)),
        Err(CephWireError::InvalidCephFsJournal { .. })
    ));

    let mut payload = Vec::new();
    append_header_payload(&mut payload, true);
    let magic_end = 4 + CEPHFS_JOURNAL_MAGIC.len();
    payload[magic_end..magic_end + 8].copy_from_slice(&0x40_0200u64.to_le_bytes());
    assert!(matches!(
        decode_cephfs_journal_header(&envelope(2, 2, &payload)),
        Err(CephWireError::InvalidCephFsJournal { .. })
    ));
}

fn nested_versioned_event(
    event_type: u32,
    nested_version: u8,
    nested_compat: u8,
    nested_payload: &[u8],
    outer_tail: &[u8],
) -> Vec<u8> {
    let mut payload = event_type.to_le_bytes().to_vec();
    payload.extend_from_slice(&envelope(nested_version, nested_compat, nested_payload));
    payload.extend_from_slice(outer_tail);
    let mut event = 0u32.to_le_bytes().to_vec();
    event.extend_from_slice(&envelope(2, 1, &payload));
    event
}

fn boundary_payload(sequence: u64, tail: &[u8]) -> Vec<u8> {
    let mut payload = sequence.to_le_bytes().to_vec();
    payload.extend_from_slice(tail);
    payload
}

fn decode_resilient(payload: &[u8]) -> ceph_wire::CephFsJournalFrame {
    decode_cephfs_journal_frame(
        &resilient_frame(payload, FRAME_OFFSET),
        FRAME_OFFSET,
        CephFsJournalStreamFormat::Resilient,
        CEPHFS_JOURNAL_MAX_EVENT_BYTES,
    )
    .unwrap()
}
