use sha2::{Digest, Sha256};

use super::{
    CephFsJournalEventRecord, CephFsJournalEventSpanRecord, CephFsJournalMapProvenanceRecord,
    CephFsJournalReplayManifest,
};

pub fn cephfs_journal_input_sha256(manifest: &CephFsJournalReplayManifest) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-journal-input/v1\0");
    manifest_identity_fields(&mut digest, manifest);
    field(&mut digest, manifest.source_semantic_sha256.as_bytes());
    field(&mut digest, manifest.metadata_inventory_sha256.as_bytes());
    for value in [
        &manifest.raw_fsmap_snapshot_sha256,
        &manifest.raw_mdsmap_snapshot_sha256,
        &manifest.map_provenance_sha256,
    ] {
        field(&mut digest, value.as_bytes());
    }
    digest.update(manifest.map_provenance_count.to_be_bytes());
    control_fields(
        &mut digest,
        &manifest.pointer_locator,
        &manifest.pointer_object_identity_sha256,
        &manifest.pointer_range_offset_hex,
        &manifest.pointer_range_length_hex,
        &manifest.pointer_range_sha256,
    );
    control_fields(
        &mut digest,
        &manifest.header_locator,
        &manifest.header_object_identity_sha256,
        &manifest.header_range_offset_hex,
        &manifest.header_range_length_hex,
        &manifest.header_range_sha256,
    );
    for value in [
        &manifest.trimmed_pos_hex,
        &manifest.expire_pos_hex,
        &manifest.unused_pos_hex,
        &manifest.write_pos_hex,
        &manifest.committed_header_tail_hex,
        &manifest.stream_format,
    ] {
        field(&mut digest, value.as_bytes());
    }
    hex::encode(digest.finalize())
}

pub fn cephfs_journal_map_provenance_sha256(
    records: &[CephFsJournalMapProvenanceRecord],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-map-provenance/v1\0");
    let mut canonical = records.iter().collect::<Vec<_>>();
    canonical.sort_by(|left, right| {
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
    for record in canonical {
        digest.update(record.rank.to_be_bytes());
        for value in [
            &record.filesystem_identity,
            &record.inventory_id,
            &record.source_identity,
            &record.source_inventory_identity,
            &record.captured_at,
            &record.raw_fsmap_snapshot_sha256,
            &record.raw_mdsmap_snapshot_sha256,
        ] {
            field(&mut digest, value.as_bytes());
        }
    }
    hex::encode(digest.finalize())
}

pub fn cephfs_journal_projection_sha256(
    manifest: &CephFsJournalReplayManifest,
    events: &[CephFsJournalEventRecord],
    spans: &[CephFsJournalEventSpanRecord],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-journal-projection/v1\0");
    manifest_identity_fields(&mut digest, manifest);
    field(&mut digest, manifest.input_sha256.as_bytes());
    field(&mut digest, manifest.consensus_replay_sha256.as_bytes());
    field(&mut digest, manifest.framing_safe_pos_hex.as_bytes());
    optional_field(
        &mut digest,
        manifest
            .namespace_safe_pos_hex
            .as_deref()
            .map(str::as_bytes),
    );
    field(&mut digest, manifest.sequence_safe_pos_hex.as_bytes());
    field(&mut digest, manifest.framing_status.as_bytes());
    optional_field(
        &mut digest,
        manifest.stop_reason.as_deref().map(str::as_bytes),
    );
    optional_field(
        &mut digest,
        manifest.namespace_stop_reason.as_deref().map(str::as_bytes),
    );
    optional_field(
        &mut digest,
        manifest.sequence_stop_reason.as_deref().map(str::as_bytes),
    );
    digest.update(manifest.event_count.to_be_bytes());

    let mut canonical_events = events.iter().collect::<Vec<_>>();
    canonical_events.sort_by_key(|event| event.event_ordinal);
    for event in canonical_events {
        event_fields(&mut digest, event);
    }

    let mut canonical_spans = spans.iter().collect::<Vec<_>>();
    canonical_spans.sort_by_key(|span| (span.event_ordinal, span.span_ordinal));
    for span in canonical_spans {
        span_fields(&mut digest, span);
    }
    hex::encode(digest.finalize())
}

fn manifest_identity_fields(digest: &mut Sha256, manifest: &CephFsJournalReplayManifest) {
    for value in [
        &manifest.filesystem_identity,
        &manifest.inventory_id,
        &manifest.data_source_id,
    ] {
        field(digest, value.as_bytes());
    }
    digest.update(manifest.rank.to_be_bytes());
    digest.update(manifest.filesystem_id.to_be_bytes());
    digest.update(manifest.fsmap_epoch.to_be_bytes());
    digest.update(manifest.mdsmap_epoch.to_be_bytes());
    digest.update(manifest.rank_incarnation.to_be_bytes());
    for value in [
        &manifest.rank_gid_hex,
        &manifest.pointer_front_inode_hex,
        &manifest.pointer_back_inode_hex,
        &manifest.journal_inode_hex,
    ] {
        field(digest, value.as_bytes());
    }
    digest.update(manifest.schema_version.to_be_bytes());
    field(digest, manifest.decoder_profile.as_bytes());
}

fn control_fields(
    digest: &mut Sha256,
    locator: &str,
    object_identity: &str,
    offset: &str,
    length: &str,
    range_sha256: &str,
) {
    for value in [locator, object_identity, offset, length, range_sha256] {
        field(digest, value.as_bytes());
    }
}

fn event_fields(digest: &mut Sha256, event: &CephFsJournalEventRecord) {
    digest.update(event.event_ordinal.to_be_bytes());
    optional_field(
        digest,
        event.segment_sequence_hex.as_deref().map(str::as_bytes),
    );
    optional_field(
        digest,
        event.event_sequence_hex.as_deref().map(str::as_bytes),
    );
    field(digest, event.sequence_disposition.as_bytes());
    for value in [&event.logical_offset_hex, &event.logical_end_hex] {
        field(digest, value.as_bytes());
    }
    digest.update(event.payload_length.to_be_bytes());
    field(digest, event.payload_sha256.as_bytes());
    digest.update(event.event_type.to_be_bytes());
    field(digest, event.event_kind.as_bytes());
    field(digest, event.event_encoding.as_bytes());
    digest.update(event.event_version.unwrap_or_default().to_be_bytes());
    digest.update(event.event_compat_version.unwrap_or_default().to_be_bytes());
    digest.update([
        u8::from(event.event_version.is_some()),
        u8::from(event.event_compat_version.is_some()),
    ]);
}

fn span_fields(digest: &mut Sha256, span: &CephFsJournalEventSpanRecord) {
    digest.update(span.event_ordinal.to_be_bytes());
    digest.update(span.span_ordinal.to_be_bytes());
    for value in [
        &span.object_locator,
        &span.object_identity_sha256,
        &span.logical_offset_hex,
        &span.object_offset_hex,
        &span.range_length_hex,
        &span.range_sha256,
    ] {
        field(digest, value.as_bytes());
    }
}

fn optional_field(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            field(digest, value);
        }
        None => digest.update([0]),
    }
}

fn field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}
