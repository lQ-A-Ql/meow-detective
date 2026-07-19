use std::collections::{HashMap, HashSet};

use chrono::{DateTime, SecondsFormat, Utc};

use super::{
    cephfs_journal_input_sha256, cephfs_journal_map_provenance_sha256,
    cephfs_journal_projection_sha256, CephFsJournalEventRecord, CephFsJournalEventSpanRecord,
    CephFsJournalMapProvenanceRecord, CephFsJournalReplayManifest, CephFsJournalReplayProjection,
    CephFsJournalRepoError, CephFsJournalRepoResult, CEPHFS_JOURNAL_DECODER_PROFILE,
    CEPHFS_JOURNAL_SCHEMA_VERSION,
};

const ZERO_U64_HEX: &str = "0000000000000000";

pub(super) fn validate_projection(
    projection: &CephFsJournalReplayProjection,
) -> CephFsJournalRepoResult<()> {
    validate_manifest(&projection.manifest)?;
    validate_map_provenance(&projection.manifest, &projection.map_provenance)?;
    super::event_validation::validate_events(&projection.manifest, &projection.events)?;
    validate_spans(&projection.manifest, &projection.events, &projection.spans)?;
    if projection.manifest.event_count != projection.events.len() as u64 {
        return invalid("manifest event count does not match framed events");
    }
    if cephfs_journal_input_sha256(&projection.manifest) != projection.manifest.input_sha256 {
        return invalid("journal input digest does not match control provenance");
    }
    if cephfs_journal_projection_sha256(&projection.manifest, &projection.events, &projection.spans)
        != projection.manifest.projection_sha256
    {
        return invalid("journal replay digest does not match its projection");
    }
    Ok(())
}

fn validate_manifest(manifest: &CephFsJournalReplayManifest) -> CephFsJournalRepoResult<()> {
    if !valid_identity(&manifest.filesystem_identity)
        || !valid_identity(&manifest.inventory_id)
        || !valid_identity(&manifest.data_source_id)
        || manifest.rank >= 0x100
        || manifest.filesystem_id < 0
        || manifest.fsmap_epoch == 0
        || manifest.mdsmap_epoch == 0
        || manifest.rank_incarnation < 0
        || manifest.schema_version != CEPHFS_JOURNAL_SCHEMA_VERSION
        || manifest.decoder_profile != CEPHFS_JOURNAL_DECODER_PROFILE
    {
        return invalid("journal manifest identity, epoch, rank, or profile is invalid");
    }
    for value in [
        &manifest.rank_gid_hex,
        &manifest.pointer_front_inode_hex,
        &manifest.pointer_back_inode_hex,
        &manifest.journal_inode_hex,
        &manifest.pointer_range_offset_hex,
        &manifest.pointer_range_length_hex,
        &manifest.header_range_offset_hex,
        &manifest.header_range_length_hex,
        &manifest.trimmed_pos_hex,
        &manifest.expire_pos_hex,
        &manifest.unused_pos_hex,
        &manifest.write_pos_hex,
        &manifest.committed_header_tail_hex,
        &manifest.framing_safe_pos_hex,
        &manifest.sequence_safe_pos_hex,
    ] {
        if !valid_u64_hex(value) {
            return invalid("journal manifest contains a non-canonical u64 value");
        }
    }
    if manifest
        .namespace_safe_pos_hex
        .as_deref()
        .is_some_and(|value| !valid_u64_hex(value))
    {
        return invalid("journal namespace boundary is not canonical");
    }
    if manifest.pointer_front_inode_hex == ZERO_U64_HEX
        || manifest.pointer_front_inode_hex != manifest.journal_inode_hex
        || !valid_journal_inodes(manifest)
        || !valid_control_range(
            &manifest.pointer_range_offset_hex,
            &manifest.pointer_range_length_hex,
        )
        || !valid_control_range(
            &manifest.header_range_offset_hex,
            &manifest.header_range_length_hex,
        )
    {
        return invalid("journal control-object range or inode binding is invalid");
    }
    if !valid_control_provenance(manifest) {
        return invalid("journal control-object provenance is invalid");
    }
    if !valid_positions(manifest) {
        return invalid("journal boundary positions are inconsistent");
    }
    if !matches!(manifest.stream_format.as_str(), "legacy" | "resilient")
        || !valid_status(manifest)
        || manifest
            .namespace_stop_reason
            .as_deref()
            .is_some_and(|value| !valid_reason(value))
        || manifest
            .sequence_stop_reason
            .as_deref()
            .is_some_and(|value| !valid_sequence_reason(value))
        || manifest.event_count > i64::MAX as u64
    {
        return invalid("journal status, stop reason, stream format, or count is invalid");
    }
    if [
        &manifest.raw_fsmap_snapshot_sha256,
        &manifest.raw_mdsmap_snapshot_sha256,
        &manifest.map_provenance_sha256,
        &manifest.input_sha256,
        &manifest.consensus_replay_sha256,
        &manifest.projection_sha256,
    ]
    .into_iter()
    .any(|value| !valid_sha256(value))
        || manifest.map_provenance_count == 0
        || manifest.map_provenance_count > i64::MAX as u64
    {
        return invalid("journal projection digest is invalid");
    }
    Ok(())
}

fn valid_journal_inodes(manifest: &CephFsJournalReplayManifest) -> bool {
    let Some(front) = parse_u64_hex(&manifest.pointer_front_inode_hex) else {
        return false;
    };
    let Some(back) = parse_u64_hex(&manifest.pointer_back_inode_hex) else {
        return false;
    };
    let rank = u64::from(manifest.rank);
    let valid_for_rank = |inode: u64| {
        inode
            .checked_sub(rank)
            .is_some_and(|base| matches!(base, 0x200 | 0x300))
    };
    valid_for_rank(front) && (back == 0 || (back != front && valid_for_rank(back)))
}

fn valid_control_provenance(manifest: &CephFsJournalReplayManifest) -> bool {
    [
        manifest.source_semantic_sha256.as_str(),
        manifest.metadata_inventory_sha256.as_str(),
        manifest.pointer_object_identity_sha256.as_str(),
        manifest.pointer_range_sha256.as_str(),
        manifest.header_object_identity_sha256.as_str(),
        manifest.header_range_sha256.as_str(),
    ]
    .into_iter()
    .all(valid_sha256)
        && valid_identity(&manifest.pointer_locator)
        && valid_identity(&manifest.header_locator)
}

fn validate_map_provenance(
    manifest: &CephFsJournalReplayManifest,
    records: &[CephFsJournalMapProvenanceRecord],
) -> CephFsJournalRepoResult<()> {
    if records.len() as u64 != manifest.map_provenance_count {
        return invalid("journal map provenance count does not match its manifest");
    }
    let mut identities = HashSet::new();
    let mut previous_key = None;
    for record in records {
        let key = (
            record.source_identity.clone(),
            record.source_inventory_identity.clone(),
            record.captured_at.clone(),
        );
        if !map_provenance_bound_to_manifest(record, manifest)
            || !valid_identity(&record.source_identity)
            || !valid_identity(&record.source_inventory_identity)
            || !valid_canonical_timestamp(&record.captured_at)
            || record.raw_fsmap_snapshot_sha256 != manifest.raw_fsmap_snapshot_sha256
            || record.raw_mdsmap_snapshot_sha256 != manifest.raw_mdsmap_snapshot_sha256
            || !identities.insert((
                record.source_identity.as_str(),
                record.source_inventory_identity.as_str(),
            ))
            || previous_key
                .as_ref()
                .is_some_and(|previous| previous >= &key)
        {
            return invalid("journal map provenance is invalid or non-canonical");
        }
        previous_key = Some(key);
    }
    if cephfs_journal_map_provenance_sha256(records) != manifest.map_provenance_sha256 {
        return invalid("journal map provenance digest does not match its records");
    }
    Ok(())
}

fn map_provenance_bound_to_manifest(
    record: &CephFsJournalMapProvenanceRecord,
    manifest: &CephFsJournalReplayManifest,
) -> bool {
    record.filesystem_identity == manifest.filesystem_identity
        && record.inventory_id == manifest.inventory_id
        && record.rank == manifest.rank
}

fn valid_canonical_timestamp(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                == value
        })
        .unwrap_or(false)
}

fn valid_positions(manifest: &CephFsJournalReplayManifest) -> bool {
    manifest.trimmed_pos_hex <= manifest.expire_pos_hex
        && manifest.committed_header_tail_hex == manifest.write_pos_hex
        && manifest.expire_pos_hex <= manifest.framing_safe_pos_hex
        && manifest.framing_safe_pos_hex <= manifest.write_pos_hex
        && manifest.expire_pos_hex <= manifest.sequence_safe_pos_hex
        && manifest.sequence_safe_pos_hex <= manifest.framing_safe_pos_hex
        && match manifest.sequence_stop_reason {
            Some(_) => manifest.sequence_safe_pos_hex < manifest.framing_safe_pos_hex,
            None => manifest.sequence_safe_pos_hex == manifest.framing_safe_pos_hex,
        }
        && manifest
            .namespace_safe_pos_hex
            .as_deref()
            .is_none_or(|position| {
                manifest.expire_pos_hex.as_str() <= position
                    && position <= manifest.framing_safe_pos_hex.as_str()
            })
}

fn valid_status(manifest: &CephFsJournalReplayManifest) -> bool {
    match manifest.framing_status.as_str() {
        "clean" => {
            manifest.stop_reason.is_none()
                && manifest.expire_pos_hex == manifest.write_pos_hex
                && manifest.framing_safe_pos_hex == manifest.write_pos_hex
                && manifest.event_count == 0
        }
        "complete_to_header_tail" => {
            manifest.stop_reason.is_none()
                && manifest.framing_safe_pos_hex == manifest.write_pos_hex
        }
        "incomplete" => {
            manifest.stop_reason.as_deref().is_some_and(valid_reason)
                && manifest.framing_safe_pos_hex < manifest.write_pos_hex
        }
        _ => false,
    }
}

fn validate_spans(
    manifest: &CephFsJournalReplayManifest,
    events: &[CephFsJournalEventRecord],
    spans: &[CephFsJournalEventSpanRecord],
) -> CephFsJournalRepoResult<()> {
    let events = events
        .iter()
        .map(|event| (event.event_ordinal, event))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<u64, Vec<&CephFsJournalEventSpanRecord>>::new();
    for span in spans {
        if !span_bound_to_manifest(span, manifest) || !valid_span(span) {
            return invalid("journal event span is invalid or crosses projection boundaries");
        }
        grouped.entry(span.event_ordinal).or_default().push(span);
    }
    for (ordinal, event) in &events {
        let Some(event_spans) = grouped.get_mut(ordinal) else {
            return invalid("journal framed event has no object provenance spans");
        };
        let event_end = parse_u64_hex(&event.logical_end_hex).ok_or(
            CephFsJournalRepoError::Invalid("journal framed event end is not canonical"),
        )?;
        event_spans.sort_by_key(|span| span.span_ordinal);
        let mut expected_offset = event.logical_offset_hex.clone();
        for (span_ordinal, span) in event_spans.iter().enumerate() {
            if span.span_ordinal != span_ordinal as u64
                || span.span_ordinal > i64::MAX as u64
                || span.logical_offset_hex != expected_offset
            {
                return invalid("journal event spans are not contiguous and canonical");
            }
            let end = parse_u64_hex(&span.logical_offset_hex)
                .zip(parse_u64_hex(&span.range_length_hex))
                .and_then(|(offset, length)| offset.checked_add(length))
                .ok_or(CephFsJournalRepoError::Invalid(
                    "journal event span range overflows",
                ))?;
            if end > event_end {
                return invalid("journal event span exceeds its framed event");
            }
            expected_offset = format!("{end:016x}");
        }
        if expected_offset != event.logical_end_hex {
            return invalid("journal event spans do not cover the framed event");
        }
    }
    if grouped.keys().any(|ordinal| !events.contains_key(ordinal)) {
        return invalid("journal event span references an unknown framed event");
    }
    Ok(())
}

fn valid_span(span: &CephFsJournalEventSpanRecord) -> bool {
    valid_identity(&span.object_locator)
        && valid_sha256(&span.object_identity_sha256)
        && valid_u64_hex(&span.logical_offset_hex)
        && valid_u64_hex(&span.object_offset_hex)
        && valid_u64_hex(&span.range_length_hex)
        && span.range_length_hex != ZERO_U64_HEX
        && valid_sha256(&span.range_sha256)
        && valid_range(&span.object_offset_hex, &span.range_length_hex)
}

fn span_bound_to_manifest(
    span: &CephFsJournalEventSpanRecord,
    manifest: &CephFsJournalReplayManifest,
) -> bool {
    span.filesystem_identity == manifest.filesystem_identity
        && span.inventory_id == manifest.inventory_id
        && span.rank == manifest.rank
}

fn valid_range(offset: &str, length: &str) -> bool {
    parse_u64_hex(offset)
        .zip(parse_u64_hex(length))
        .is_some_and(|(offset, length)| offset.checked_add(length).is_some())
}

fn valid_control_range(offset: &str, length: &str) -> bool {
    offset == ZERO_U64_HEX
        && parse_u64_hex(length).is_some_and(|length| (1..=64 * 1024).contains(&length))
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

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_reason(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_sequence_reason(value: &str) -> bool {
    matches!(
        value,
        "conflict" | "unknown_event" | "unsupported_semantics" | "overflow"
    )
}

fn invalid<T>(message: &'static str) -> CephFsJournalRepoResult<T> {
    Err(CephFsJournalRepoError::Invalid(message))
}
