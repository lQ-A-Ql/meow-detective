use crate::{
    codec::{CephDecode, CephStructEnvelope},
    cursor::CephCursor,
};

use super::{
    CephFsJournalBoundarySequence, CephFsJournalEvent, CephFsJournalEventEncoding,
    CephFsJournalEventKind, CephFsJournalEventSemanticState,
};

const EVENT_NEW_ENCODING: u32 = 0;

pub(super) fn decode_event(input: &[u8]) -> CephFsJournalEvent {
    let mut cursor = CephCursor::new(input);
    let Ok(marker) = u32::decode(&mut cursor) else {
        return malformed_event("event header");
    };
    if marker != EVENT_NEW_ENCODING {
        return decode_legacy_event(marker);
    }
    decode_versioned_event(&mut cursor)
}

fn decode_legacy_event(event_type: u32) -> CephFsJournalEvent {
    let kind = CephFsJournalEventKind::from_type(event_type);
    let (semantic_state, boundary_sequence) = if kind == CephFsJournalEventKind::Unknown {
        (
            CephFsJournalEventSemanticState::UnknownEventType,
            CephFsJournalBoundarySequence::NotBoundary,
        )
    } else if kind.is_boundary() {
        (
            CephFsJournalEventSemanticState::UnsupportedEnvelope {
                structure: "legacy segment boundary",
                encoded_version: 0,
                compat_version: 0,
            },
            CephFsJournalBoundarySequence::Unavailable,
        )
    } else {
        (
            CephFsJournalEventSemanticState::Supported,
            CephFsJournalBoundarySequence::NotBoundary,
        )
    };
    event(
        CephFsJournalEventEncoding::Legacy,
        kind,
        event_type,
        semantic_state,
        boundary_sequence,
    )
}

fn decode_versioned_event(cursor: &mut CephCursor<'_>) -> CephFsJournalEvent {
    let Ok(envelope) = CephStructEnvelope::decode(cursor) else {
        return malformed_event("event envelope");
    };
    let encoding = CephFsJournalEventEncoding::Versioned {
        version: envelope.version,
        compat_version: envelope.compat_version,
    };
    let Ok(mut payload) = cursor.take(envelope.payload_length as usize) else {
        return event(
            encoding,
            CephFsJournalEventKind::Unknown,
            EVENT_NEW_ENCODING,
            CephFsJournalEventSemanticState::MalformedEnvelope {
                structure: "event envelope",
            },
            CephFsJournalBoundarySequence::Unavailable,
        );
    };
    if !cursor.is_empty() {
        return event(
            encoding,
            CephFsJournalEventKind::Unknown,
            EVENT_NEW_ENCODING,
            CephFsJournalEventSemanticState::MalformedEnvelope {
                structure: "event envelope",
            },
            CephFsJournalBoundarySequence::Unavailable,
        );
    }
    if envelope.version < 1 || envelope.compat_version > 1 {
        return event(
            encoding,
            CephFsJournalEventKind::Unknown,
            EVENT_NEW_ENCODING,
            CephFsJournalEventSemanticState::UnsupportedEnvelope {
                structure: "event",
                encoded_version: envelope.version,
                compat_version: envelope.compat_version,
            },
            CephFsJournalBoundarySequence::Unavailable,
        );
    }
    let Ok(event_type) = u32::decode(&mut payload) else {
        return event(
            encoding,
            CephFsJournalEventKind::Unknown,
            EVENT_NEW_ENCODING,
            CephFsJournalEventSemanticState::MalformedEnvelope {
                structure: "event payload",
            },
            CephFsJournalBoundarySequence::Unavailable,
        );
    };
    let kind = CephFsJournalEventKind::from_type(event_type);
    let (semantic_state, boundary_sequence) = decode_semantics(kind, &mut payload);
    event(
        encoding,
        kind,
        event_type,
        semantic_state,
        boundary_sequence,
    )
}

fn decode_semantics(
    kind: CephFsJournalEventKind,
    payload: &mut CephCursor<'_>,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    match kind {
        CephFsJournalEventKind::Unknown => (
            CephFsJournalEventSemanticState::UnknownEventType,
            CephFsJournalBoundarySequence::NotBoundary,
        ),
        CephFsJournalEventKind::SubtreeMap => decode_subtree_map(payload),
        CephFsJournalEventKind::ResetJournal => decode_offset_boundary(payload, "reset journal", 2),
        CephFsJournalEventKind::Segment => decode_sequence_boundary(payload, "segment"),
        CephFsJournalEventKind::Lid => decode_sequence_boundary(payload, "lid"),
        _ => (
            CephFsJournalEventSemanticState::Supported,
            CephFsJournalBoundarySequence::NotBoundary,
        ),
    }
}

fn decode_subtree_map(
    cursor: &mut CephCursor<'_>,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    let Some((envelope, payload)) = nested_envelope(cursor) else {
        return malformed_boundary("subtree map");
    };
    if envelope.compat_version > 5 || envelope.version < 5 {
        return unsupported_boundary("subtree map", envelope);
    }
    match envelope.version {
        5 => (
            CephFsJournalEventSemanticState::Supported,
            CephFsJournalBoundarySequence::LogicalOffset,
        ),
        6 if payload.remaining() >= 8 => {
            let sequence_offset = payload.remaining() - 8;
            let mut sequence = CephCursor::new(&payload.input()[sequence_offset..]);
            match u64::decode(&mut sequence) {
                Ok(value) => (
                    CephFsJournalEventSemanticState::Supported,
                    CephFsJournalBoundarySequence::Encoded(value),
                ),
                Err(_) => malformed_boundary("subtree map"),
            }
        }
        6 => malformed_boundary("subtree map"),
        _ => (
            CephFsJournalEventSemanticState::UnsupportedEnvelope {
                structure: "subtree map sequence",
                encoded_version: envelope.version,
                compat_version: envelope.compat_version,
            },
            CephFsJournalBoundarySequence::Unavailable,
        ),
    }
}

fn decode_offset_boundary(
    cursor: &mut CephCursor<'_>,
    structure: &'static str,
    minimum_version: u8,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    let Some((envelope, payload)) = nested_envelope(cursor) else {
        return malformed_boundary(structure);
    };
    if envelope.compat_version > minimum_version || envelope.version < minimum_version {
        return unsupported_boundary(structure, envelope);
    }
    if payload.remaining() < 8 {
        return malformed_boundary(structure);
    }
    (
        CephFsJournalEventSemanticState::Supported,
        CephFsJournalBoundarySequence::LogicalOffset,
    )
}

fn decode_sequence_boundary(
    cursor: &mut CephCursor<'_>,
    structure: &'static str,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    let Some((envelope, mut payload)) = nested_envelope(cursor) else {
        return malformed_boundary(structure);
    };
    if envelope.compat_version > 1 || envelope.version < 1 {
        return unsupported_boundary(structure, envelope);
    }
    match u64::decode(&mut payload) {
        Ok(sequence) => (
            CephFsJournalEventSemanticState::Supported,
            CephFsJournalBoundarySequence::Encoded(sequence),
        ),
        Err(_) => malformed_boundary(structure),
    }
}

fn nested_envelope<'a>(
    cursor: &mut CephCursor<'a>,
) -> Option<(CephStructEnvelope, CephCursor<'a>)> {
    let envelope = CephStructEnvelope::decode(cursor).ok()?;
    let payload = cursor.take(envelope.payload_length as usize).ok()?;
    Some((envelope, payload))
}

fn unsupported_boundary(
    structure: &'static str,
    envelope: CephStructEnvelope,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    (
        CephFsJournalEventSemanticState::UnsupportedEnvelope {
            structure,
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        },
        CephFsJournalBoundarySequence::Unavailable,
    )
}

fn malformed_boundary(
    structure: &'static str,
) -> (
    CephFsJournalEventSemanticState,
    CephFsJournalBoundarySequence,
) {
    (
        CephFsJournalEventSemanticState::MalformedEnvelope { structure },
        CephFsJournalBoundarySequence::Unavailable,
    )
}

fn malformed_event(structure: &'static str) -> CephFsJournalEvent {
    event(
        CephFsJournalEventEncoding::Legacy,
        CephFsJournalEventKind::Unknown,
        EVENT_NEW_ENCODING,
        CephFsJournalEventSemanticState::MalformedEnvelope { structure },
        CephFsJournalBoundarySequence::Unavailable,
    )
}

fn event(
    encoding: CephFsJournalEventEncoding,
    kind: CephFsJournalEventKind,
    event_type: u32,
    semantic_state: CephFsJournalEventSemanticState,
    boundary_sequence: CephFsJournalBoundarySequence,
) -> CephFsJournalEvent {
    let segment_sequence = match boundary_sequence {
        CephFsJournalBoundarySequence::Encoded(sequence) => Some(sequence),
        _ => None,
    };
    CephFsJournalEvent {
        encoding,
        kind,
        event_type,
        semantic_state,
        boundary_sequence,
        segment_sequence,
    }
}
