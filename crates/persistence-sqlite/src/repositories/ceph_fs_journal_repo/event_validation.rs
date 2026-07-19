use super::{
    CephFsJournalEventRecord, CephFsJournalReplayManifest, CephFsJournalRepoError,
    CephFsJournalRepoResult,
};

pub(super) fn validate_events(
    manifest: &CephFsJournalReplayManifest,
    events: &[CephFsJournalEventRecord],
) -> CephFsJournalRepoResult<()> {
    let mut canonical = events.iter().collect::<Vec<_>>();
    canonical.sort_by_key(|event| event.event_ordinal);
    let mut expected_offset = manifest.expire_pos_hex.clone();
    let mut sequence = SequenceValidationState::default();
    for (ordinal, event) in canonical.into_iter().enumerate() {
        if !event_bound_to_manifest(event, manifest)
            || event.event_ordinal != ordinal as u64
            || event.event_ordinal > i64::MAX as u64
            || !valid_event(event, &manifest.stream_format)
            || event.logical_offset_hex != expected_offset
            || event.logical_end_hex > manifest.framing_safe_pos_hex
            || !sequence.observe(event)
        {
            return invalid("journal framed event ordering or binding is invalid");
        }
        expected_offset.clone_from(&event.logical_end_hex);
    }
    if expected_offset != manifest.framing_safe_pos_hex {
        return invalid("journal safe boundary does not match the final framed event");
    }
    if !sequence.matches_manifest(manifest) {
        return invalid("journal sequence boundary does not match framed events");
    }
    Ok(())
}

#[derive(Default)]
struct SequenceValidationState {
    current_segment: Option<u64>,
    previous_event: Option<u64>,
    semantic_unavailable: bool,
    stop_offset: Option<String>,
    stop_class: Option<SequenceStopClass>,
}

#[derive(Clone, Copy)]
enum SequenceStopClass {
    UnknownOrUnsupported,
    BoundaryConflictOrUnsupported,
    Overflow,
}

impl SequenceValidationState {
    fn observe(&mut self, event: &CephFsJournalEventRecord) -> bool {
        match event.sequence_disposition.as_str() {
            "semantic_unavailable" => self.observe_unavailable(event),
            "ignored_lid" => self.observe_ignored_lid(event),
            "resolved" => self.observe_resolved(event),
            _ => false,
        }
    }

    fn observe_unavailable(&mut self, event: &CephFsJournalEventRecord) -> bool {
        if event.segment_sequence_hex.is_some() || event.event_sequence_hex.is_some() {
            return false;
        }
        if !self.semantic_unavailable {
            let stop_class = if event.event_kind == "unknown" {
                SequenceStopClass::UnknownOrUnsupported
            } else if is_boundary_type(event.event_type) {
                SequenceStopClass::BoundaryConflictOrUnsupported
            } else if self.previous_event == Some(u64::MAX) {
                SequenceStopClass::Overflow
            } else {
                return false;
            };
            self.stop_offset = Some(event.logical_offset_hex.clone());
            self.stop_class = Some(stop_class);
        }
        self.semantic_unavailable = true;
        true
    }

    fn observe_ignored_lid(&mut self, event: &CephFsJournalEventRecord) -> bool {
        event.event_type == 101
            && event.event_kind == "lid"
            && !self.semantic_unavailable
            && self.current_segment.is_some()
            && self.previous_event.is_some()
            && event.segment_sequence_hex.is_none()
            && event.event_sequence_hex.is_none()
    }

    fn observe_resolved(&mut self, event: &CephFsJournalEventRecord) -> bool {
        if self.semantic_unavailable {
            return false;
        }
        let Some(event_sequence) = event.event_sequence_hex.as_deref().and_then(parse_u64_hex)
        else {
            return false;
        };
        if is_boundary_type(event.event_type) {
            return self.observe_boundary(event, event_sequence);
        }
        let Some(segment) = parse_optional_u64_hex(event.segment_sequence_hex.as_deref()) else {
            return false;
        };
        if segment != self.current_segment {
            return false;
        }
        let expected = self
            .previous_event
            .map_or(Some(1), |value| value.checked_add(1));
        if expected != Some(event_sequence) {
            return false;
        }
        self.previous_event = Some(event_sequence);
        true
    }

    fn observe_boundary(&mut self, event: &CephFsJournalEventRecord, event_sequence: u64) -> bool {
        if event.event_type == 101 && self.current_segment.is_some() {
            return false;
        }
        let Some(segment) = event
            .segment_sequence_hex
            .as_deref()
            .and_then(parse_u64_hex)
        else {
            return false;
        };
        if event_sequence != segment
            || self
                .current_segment
                .is_some_and(|previous| segment <= previous)
        {
            return false;
        }
        self.current_segment = Some(segment);
        self.previous_event = Some(event_sequence);
        true
    }

    fn matches_manifest(&self, manifest: &CephFsJournalReplayManifest) -> bool {
        let expected_safe = self
            .stop_offset
            .as_deref()
            .unwrap_or(&manifest.framing_safe_pos_hex);
        if manifest.sequence_safe_pos_hex != expected_safe {
            return false;
        }
        matches!(
            (self.stop_class, manifest.sequence_stop_reason.as_deref()),
            (None, None)
                | (
                    Some(SequenceStopClass::UnknownOrUnsupported),
                    Some("unknown_event" | "unsupported_semantics"),
                )
                | (
                    Some(SequenceStopClass::BoundaryConflictOrUnsupported),
                    Some("conflict" | "unsupported_semantics"),
                )
                | (Some(SequenceStopClass::Overflow), Some("overflow"))
        )
    }
}

fn valid_event(event: &CephFsJournalEventRecord, stream_format: &str) -> bool {
    if !valid_optional_u64_hex(event.event_sequence_hex.as_deref())
        || !valid_u64_hex(&event.logical_offset_hex)
        || !valid_u64_hex(&event.logical_end_hex)
        || event.logical_offset_hex >= event.logical_end_hex
        || event
            .segment_sequence_hex
            .as_deref()
            .is_some_and(|value| !valid_u64_hex(value))
        || !valid_sha256(&event.payload_sha256)
        || event.event_kind != event_kind_for_type(event.event_type)
    {
        return false;
    }
    let frame_overhead = match stream_format {
        "legacy" => 4,
        "resilient" => 20,
        _ => return false,
    };
    let frame_length = parse_u64_hex(&event.logical_end_hex)
        .zip(parse_u64_hex(&event.logical_offset_hex))
        .and_then(|(end, start)| end.checked_sub(start));
    if frame_length != Some(u64::from(event.payload_length) + frame_overhead) {
        return false;
    }
    match event.event_encoding.as_str() {
        "legacy" => event.event_version.is_none() && event.event_compat_version.is_none(),
        "versioned" => matches!(
            (event.event_version, event.event_compat_version),
            (Some(version), Some(compat)) if version > 0 && compat <= 1 && compat <= version
        ),
        _ => false,
    }
}

fn event_bound_to_manifest(
    event: &CephFsJournalEventRecord,
    manifest: &CephFsJournalReplayManifest,
) -> bool {
    event.filesystem_identity == manifest.filesystem_identity
        && event.inventory_id == manifest.inventory_id
        && event.rank == manifest.rank
}

fn valid_optional_u64_hex(value: Option<&str>) -> bool {
    value.is_none_or(valid_u64_hex)
}

fn parse_optional_u64_hex(value: Option<&str>) -> Option<Option<u64>> {
    match value {
        Some(value) => parse_u64_hex(value).map(Some),
        None => Some(None),
    }
}

fn is_boundary_type(event_type: u32) -> bool {
    matches!(event_type, 2 | 9 | 100 | 101)
}

fn event_kind_for_type(event_type: u32) -> &'static str {
    match event_type {
        2 => "subtree_map",
        3 => "export",
        4 => "import_start",
        5 => "import_finish",
        6 => "fragment",
        9 => "reset_journal",
        10 => "session",
        11 => "sessions_old",
        12 => "sessions",
        20 => "update",
        21 => "peer_update",
        22 => "open",
        23 => "committed",
        24 => "purged",
        42 => "table_client",
        43 => "table_server",
        50 => "subtree_map_test",
        51 => "noop",
        100 => "segment",
        101 => "lid",
        _ => "unknown",
    }
}

fn parse_u64_hex(value: &str) -> Option<u64> {
    valid_u64_hex(value).then(|| u64::from_str_radix(value, 16).ok())?
}

fn valid_u64_hex(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid<T>(message: &'static str) -> CephFsJournalRepoResult<T> {
    Err(CephFsJournalRepoError::Invalid(message))
}
