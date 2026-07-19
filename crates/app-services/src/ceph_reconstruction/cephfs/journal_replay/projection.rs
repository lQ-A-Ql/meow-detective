use ceph_wire::CephFsJournalEventEncoding;
use chrono::SecondsFormat;
use persistence_sqlite::repositories::{
    ceph_fs_journal_repo::{
        cephfs_journal_input_sha256, cephfs_journal_map_provenance_sha256,
        cephfs_journal_projection_sha256, cephfs_journal_u64_hex, CephFsJournalEventRecord,
        CephFsJournalEventSpanRecord, CephFsJournalMapProvenanceRecord,
        CephFsJournalReplayManifest, CephFsJournalReplayProjection, CEPHFS_JOURNAL_DECODER_PROFILE,
        CEPHFS_JOURNAL_SCHEMA_VERSION,
    },
    ceph_fs_metadata_inventory_repo::CephFsMetadataInventoryManifest,
};

use super::super::CephFsDescriptor;
use super::{
    digest, types::CephFsJournalSequenceStatus, CephFsJournalPersistenceError, CephFsJournalReplay,
    CephFsJournalSourceSpan,
};

pub(super) fn build_projection(
    replay: &CephFsJournalReplay,
    descriptor: &CephFsDescriptor,
    metadata: &CephFsMetadataInventoryManifest,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<CephFsJournalReplayProjection, CephFsJournalPersistenceError> {
    validate_binding(replay, descriptor, metadata, data_source_id, inventory_id)?;
    let pointer = control_span(
        &replay.pointer_spans,
        data_source_id,
        inventory_id,
        "pointer",
    )?;
    let header = control_span(&replay.header_spans, data_source_id, inventory_id, "header")?;
    let map_provenance = project_map_provenance(replay, descriptor, inventory_id);
    let map_provenance_sha256 = cephfs_journal_map_provenance_sha256(&map_provenance);
    let canonical_map = &descriptor.provenance[0];
    let events = project_events(replay, inventory_id);
    let spans = project_spans(replay, data_source_id, inventory_id)?;
    let sequence_safe_pos = replay
        .sequence_safe_pos
        .ok_or(CephFsJournalPersistenceError::InvalidProjection)?;
    let mut manifest = CephFsJournalReplayManifest {
        filesystem_identity: replay.filesystem_identity.clone(),
        inventory_id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        rank: replay.rank,
        filesystem_id: metadata.filesystem_id,
        fsmap_epoch: replay.fsmap_epoch,
        mdsmap_epoch: replay.mdsmap_epoch,
        rank_incarnation: replay.rank_incarnation,
        rank_gid_hex: cephfs_journal_u64_hex(replay.rank_gid),
        pointer_front_inode_hex: cephfs_journal_u64_hex(replay.pointer.front),
        pointer_back_inode_hex: cephfs_journal_u64_hex(replay.pointer.back),
        journal_inode_hex: cephfs_journal_u64_hex(replay.pointer.front),
        schema_version: CEPHFS_JOURNAL_SCHEMA_VERSION,
        decoder_profile: CEPHFS_JOURNAL_DECODER_PROFILE.to_string(),
        source_semantic_sha256: metadata.source_semantic_sha256.clone(),
        metadata_inventory_sha256: metadata.inventory_sha256.clone(),
        raw_fsmap_snapshot_sha256: canonical_map.raw_fsmap_sha256.clone(),
        raw_mdsmap_snapshot_sha256: canonical_map.raw_mdsmap_sha256.clone(),
        map_provenance_sha256,
        map_provenance_count: map_provenance.len() as u64,
        pointer_locator: pointer.span.locator.clone(),
        pointer_object_identity_sha256: pointer.object_identity_sha256,
        pointer_range_offset_hex: cephfs_journal_u64_hex(pointer.span.object_offset),
        pointer_range_length_hex: cephfs_journal_u64_hex(pointer.span.length),
        pointer_range_sha256: pointer.span.range_sha256.clone(),
        header_locator: header.span.locator.clone(),
        header_object_identity_sha256: header.object_identity_sha256,
        header_range_offset_hex: cephfs_journal_u64_hex(header.span.object_offset),
        header_range_length_hex: cephfs_journal_u64_hex(header.span.length),
        header_range_sha256: header.span.range_sha256.clone(),
        trimmed_pos_hex: cephfs_journal_u64_hex(replay.header.trimmed_pos),
        expire_pos_hex: cephfs_journal_u64_hex(replay.header.expire_pos),
        unused_pos_hex: cephfs_journal_u64_hex(replay.header.unused_pos),
        write_pos_hex: cephfs_journal_u64_hex(replay.header.write_pos),
        committed_header_tail_hex: cephfs_journal_u64_hex(replay.committed_header_tail),
        framing_safe_pos_hex: cephfs_journal_u64_hex(replay.framing_safe_pos),
        namespace_safe_pos_hex: replay.namespace_safe_pos.map(cephfs_journal_u64_hex),
        sequence_safe_pos_hex: cephfs_journal_u64_hex(sequence_safe_pos),
        stream_format: replay.header.stream_format.as_str().to_string(),
        framing_status: replay.framing_status.as_str().to_string(),
        stop_reason: replay.stop_reason.map(|reason| reason.as_str().to_string()),
        namespace_stop_reason: replay
            .namespace_stop_reason
            .map(|reason| reason.as_str().to_string()),
        sequence_stop_reason: replay
            .sequence_stop_reason
            .map(|reason| reason.as_str().to_string()),
        event_count: events.len() as u64,
        input_sha256: String::new(),
        consensus_replay_sha256: replay.replay_sha256.clone(),
        projection_sha256: String::new(),
    };
    manifest.input_sha256 = cephfs_journal_input_sha256(&manifest);
    manifest.projection_sha256 = cephfs_journal_projection_sha256(&manifest, &events, &spans);
    Ok(CephFsJournalReplayProjection {
        manifest,
        map_provenance,
        events,
        spans,
    })
}

struct LocalSpan<'a> {
    span: &'a CephFsJournalSourceSpan,
    object_identity_sha256: String,
}

fn control_span<'a>(
    spans: &'a [CephFsJournalSourceSpan],
    data_source_id: &str,
    inventory_id: &str,
    kind: &'static str,
) -> Result<LocalSpan<'a>, CephFsJournalPersistenceError> {
    if spans.len() != 1 {
        return Err(CephFsJournalPersistenceError::InvalidControlProvenance { kind });
    }
    let span = &spans[0];
    if span.logical_offset != 0
        || span.object_offset != 0
        || span.length == 0
        || span.length > 64 * 1024
    {
        return Err(CephFsJournalPersistenceError::InvalidControlProvenance { kind });
    }
    let object_identity_sha256 = local_object_identity(span, data_source_id, inventory_id)?;
    Ok(LocalSpan {
        span,
        object_identity_sha256,
    })
}

fn project_events(
    replay: &CephFsJournalReplay,
    inventory_id: &str,
) -> Vec<CephFsJournalEventRecord> {
    let mut sequence = ProjectionSequenceState::default();
    replay
        .events
        .iter()
        .map(|event| {
            let (event_version, event_compat_version) = match event.frame.event.encoding {
                CephFsJournalEventEncoding::Legacy => (None, None),
                CephFsJournalEventEncoding::Versioned {
                    version,
                    compat_version,
                } => (Some(version), Some(compat_version)),
            };
            let (segment_sequence, event_sequence, sequence_disposition) = sequence.project(
                event.frame.event.event_type,
                event.rank_local_segment_sequence,
                event.rank_local_event_sequence,
                event.sequence_status,
            );
            CephFsJournalEventRecord {
                filesystem_identity: replay.filesystem_identity.clone(),
                inventory_id: inventory_id.to_string(),
                rank: replay.rank,
                event_ordinal: event.ordinal,
                segment_sequence_hex: segment_sequence.map(cephfs_journal_u64_hex),
                event_sequence_hex: event_sequence.map(cephfs_journal_u64_hex),
                sequence_disposition: sequence_disposition.to_string(),
                logical_offset_hex: cephfs_journal_u64_hex(event.frame.logical_offset),
                logical_end_hex: cephfs_journal_u64_hex(event.frame.logical_end),
                payload_length: event.frame.payload_length,
                payload_sha256: event.frame.payload_sha256.clone(),
                event_type: event.frame.event.event_type,
                event_kind: event.frame.event.kind.as_str().to_string(),
                event_encoding: event.frame.event.encoding.as_str().to_string(),
                event_version,
                event_compat_version,
            }
        })
        .collect()
}

#[derive(Default)]
struct ProjectionSequenceState {
    semantic_unavailable: bool,
    current_segment: Option<u64>,
}

impl ProjectionSequenceState {
    fn project(
        &mut self,
        event_type: u32,
        segment_sequence: Option<u64>,
        event_sequence: u64,
        status: CephFsJournalSequenceStatus,
    ) -> (Option<u64>, Option<u64>, &'static str) {
        if status == CephFsJournalSequenceStatus::IgnoredNonInitialLid {
            if !self.semantic_unavailable && event_type == 101 && self.current_segment.is_some() {
                return (None, None, "ignored_lid");
            }
            self.semantic_unavailable = true;
            return (None, None, "semantic_unavailable");
        }
        let boundary = matches!(event_type, 2 | 9 | 100 | 101);
        if status != CephFsJournalSequenceStatus::Validated
            || self.semantic_unavailable
            || (boundary && segment_sequence != Some(event_sequence))
            || (!boundary && segment_sequence != self.current_segment)
            || (event_type == 101 && self.current_segment.is_some())
        {
            self.semantic_unavailable = true;
            return (None, None, "semantic_unavailable");
        }
        if boundary {
            self.current_segment = segment_sequence;
        }
        (segment_sequence, Some(event_sequence), "resolved")
    }
}

fn project_map_provenance(
    replay: &CephFsJournalReplay,
    descriptor: &CephFsDescriptor,
    inventory_id: &str,
) -> Vec<CephFsJournalMapProvenanceRecord> {
    let mut records = descriptor
        .provenance
        .iter()
        .map(|source| CephFsJournalMapProvenanceRecord {
            filesystem_identity: replay.filesystem_identity.clone(),
            inventory_id: inventory_id.to_string(),
            rank: replay.rank,
            source_identity: source.source_identity.clone(),
            source_inventory_identity: source.inventory_identity.clone(),
            captured_at: source
                .captured_at
                .to_rfc3339_opts(SecondsFormat::Nanos, true),
            raw_fsmap_snapshot_sha256: source.raw_fsmap_sha256.clone(),
            raw_mdsmap_snapshot_sha256: source.raw_mdsmap_sha256.clone(),
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        (
            &left.source_identity,
            &left.source_inventory_identity,
            &left.captured_at,
        )
            .cmp(&(
                &right.source_identity,
                &right.source_inventory_identity,
                &right.captured_at,
            ))
    });
    records
}

fn project_spans(
    replay: &CephFsJournalReplay,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<Vec<CephFsJournalEventSpanRecord>, CephFsJournalPersistenceError> {
    let mut projected = Vec::new();
    for event in &replay.events {
        for (span_ordinal, span) in event.spans.iter().enumerate() {
            projected.push(CephFsJournalEventSpanRecord {
                filesystem_identity: replay.filesystem_identity.clone(),
                inventory_id: inventory_id.to_string(),
                rank: replay.rank,
                event_ordinal: event.ordinal,
                span_ordinal: span_ordinal as u64,
                object_locator: span.locator.clone(),
                object_identity_sha256: local_object_identity(span, data_source_id, inventory_id)?,
                logical_offset_hex: cephfs_journal_u64_hex(span.logical_offset),
                object_offset_hex: cephfs_journal_u64_hex(span.object_offset),
                range_length_hex: cephfs_journal_u64_hex(span.length),
                range_sha256: span.range_sha256.clone(),
            });
        }
    }
    Ok(projected)
}

fn local_object_identity(
    span: &CephFsJournalSourceSpan,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<String, CephFsJournalPersistenceError> {
    let mut local = span.provenance.iter().filter(|source| {
        source.data_source_id == data_source_id && source.inventory_id == inventory_id
    });
    let identity =
        local
            .next()
            .ok_or_else(|| CephFsJournalPersistenceError::MissingLocalProvenance {
                locator: span.locator.clone(),
            })?;
    if local.next().is_some() {
        return Err(CephFsJournalPersistenceError::DuplicateLocalProvenance {
            locator: span.locator.clone(),
        });
    }
    Ok(identity.object_identity_sha256.clone())
}

fn validate_binding(
    replay: &CephFsJournalReplay,
    descriptor: &CephFsDescriptor,
    metadata: &CephFsMetadataInventoryManifest,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<(), CephFsJournalPersistenceError> {
    let replay_digest_matches = replay.replay_sha256 == digest::replay_sha256(replay);
    if data_source_id.trim().is_empty()
        || inventory_id.trim().is_empty()
        || data_source_id.contains('\0')
        || inventory_id.contains('\0')
        || replay.rank >= 0x100
        || !valid_descriptor_binding(replay, descriptor)
        || !replay_digest_matches
    {
        return Err(if !replay_digest_matches {
            CephFsJournalPersistenceError::ReplayDigestMismatch
        } else {
            CephFsJournalPersistenceError::InvalidSourceBinding
        });
    }
    if !metadata.complete
        || metadata.filesystem_identity != replay.filesystem_identity
        || metadata.inventory_id != inventory_id
        || metadata.data_source_id != data_source_id
        || metadata.fsmap_epoch != replay.fsmap_epoch
        || metadata.metadata_pool_id != replay.header.layout.pool_id
        || metadata.inventory_sha256.len() != 64
        || metadata.source_semantic_sha256.len() != 64
    {
        return Err(CephFsJournalPersistenceError::MetadataInventoryUnavailable);
    }
    Ok(())
}

fn valid_descriptor_binding(replay: &CephFsJournalReplay, descriptor: &CephFsDescriptor) -> bool {
    if descriptor.identity != replay.filesystem_identity
        || descriptor.filesystem_id < 0
        || descriptor.fsmap_epoch != replay.fsmap_epoch
        || descriptor.mdsmap_epoch != replay.mdsmap_epoch
        || descriptor.metadata_pool.pool_id != replay.header.layout.pool_id
        || descriptor.provenance.is_empty()
        || !descriptor.rank_bindings.iter().any(|binding| {
            binding.rank == replay.rank
                && binding.gid == replay.rank_gid
                && binding.incarnation == replay.rank_incarnation
        })
    {
        return false;
    }
    let first = &descriptor.provenance[0];
    descriptor.provenance.iter().all(|source| {
        valid_identity(&source.source_identity)
            && valid_identity(&source.inventory_identity)
            && valid_sha256(&source.raw_fsmap_sha256)
            && valid_sha256(&source.raw_mdsmap_sha256)
            && source.raw_fsmap_sha256 == first.raw_fsmap_sha256
            && source.raw_mdsmap_sha256 == first.raw_mdsmap_sha256
    }) && descriptor.provenance.windows(2).all(|pair| {
        (
            &pair[0].source_identity,
            &pair[0].inventory_identity,
            pair[0].captured_at,
        ) < (
            &pair[1].source_identity,
            &pair[1].inventory_identity,
            pair[1].captured_at,
        )
    })
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
