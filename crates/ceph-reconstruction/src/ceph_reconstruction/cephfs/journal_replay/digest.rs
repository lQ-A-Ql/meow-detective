use sha2::{Digest, Sha256};

use super::CephFsJournalReplay;

pub(super) fn replay_sha256(replay: &CephFsJournalReplay) -> String {
    let mut digest = Sha256::new();
    field(&mut digest, &replay.filesystem_identity);
    field(&mut digest, &replay.fsmap_epoch.to_string());
    field(&mut digest, &replay.mdsmap_epoch.to_string());
    field(&mut digest, &replay.rank.to_string());
    field(&mut digest, &format!("{:016x}", replay.rank_gid));
    field(&mut digest, &replay.rank_incarnation.to_string());
    field(&mut digest, &format!("{:016x}", replay.pointer.front));
    field(&mut digest, &format!("{:016x}", replay.pointer.back));
    header_fields(&mut digest, replay);
    source_spans(&mut digest, &replay.pointer_spans);
    source_spans(&mut digest, &replay.header_spans);
    field(&mut digest, replay.framing_status.as_str());
    field(
        &mut digest,
        &replay
            .namespace_safe_pos
            .map(|value| format!("{value:016x}"))
            .unwrap_or_else(|| "none".to_string()),
    );
    field(&mut digest, &format!("{:016x}", replay.framing_safe_pos));
    field(
        &mut digest,
        replay
            .stop_reason
            .map(|reason| reason.as_str())
            .unwrap_or("none"),
    );
    field(
        &mut digest,
        &replay
            .sequence_safe_pos
            .map(|value| format!("{value:016x}"))
            .unwrap_or_else(|| "none".to_string()),
    );
    field(
        &mut digest,
        replay
            .sequence_stop_reason
            .map(|reason| reason.as_str())
            .unwrap_or("none"),
    );
    field(
        &mut digest,
        replay
            .namespace_stop_reason
            .map(|reason| reason.as_str())
            .unwrap_or("none"),
    );
    digest.update((replay.events.len() as u64).to_le_bytes());
    for event in &replay.events {
        field(&mut digest, &event.ordinal.to_string());
        field(
            &mut digest,
            &event
                .rank_local_segment_sequence
                .map(|value| format!("{value:016x}"))
                .unwrap_or_else(|| "none".to_string()),
        );
        field(&mut digest, &event.rank_local_event_sequence.to_string());
        field(&mut digest, event.sequence_status.as_str());
        field(&mut digest, &format!("{:016x}", event.frame.logical_offset));
        field(&mut digest, &format!("{:016x}", event.frame.logical_end));
        field(&mut digest, &event.frame.event.event_type.to_string());
        field(&mut digest, &event.frame.payload_sha256);
        field(&mut digest, event.frame.event.encoding.as_str());
        field(&mut digest, event.frame.event.kind.as_str());
        field(&mut digest, event.frame.event.semantic_state.as_str());
        field(&mut digest, event.frame.event.boundary_sequence.as_str());
        if let Some(sequence) = event.frame.event.segment_sequence {
            field(&mut digest, &format!("{sequence:016x}"));
        }
        source_spans(&mut digest, &event.spans);
    }
    format!("{:x}", digest.finalize())
}

fn header_fields(digest: &mut Sha256, replay: &CephFsJournalReplay) {
    let header = &replay.header;
    for value in [
        header.magic.clone(),
        format!("{:016x}", header.trimmed_pos),
        format!("{:016x}", header.expire_pos),
        format!("{:016x}", header.unused_pos),
        format!("{:016x}", header.write_pos),
        format!("{:016x}", replay.committed_header_tail),
        header.stream_format.as_str().to_string(),
    ] {
        field(digest, &value);
    }
    digest.update(header.layout.stripe_unit.to_le_bytes());
    digest.update(header.layout.stripe_count.to_le_bytes());
    digest.update(header.layout.object_size.to_le_bytes());
    digest.update(header.layout.pool_id.to_le_bytes());
}

fn source_spans(digest: &mut Sha256, spans: &[super::CephFsJournalSourceSpan]) {
    digest.update((spans.len() as u64).to_le_bytes());
    for span in spans {
        field(digest, &span.locator);
        field(digest, &format!("{:016x}", span.logical_offset));
        field(digest, &format!("{:016x}", span.object_offset));
        field(digest, &span.length.to_string());
        field(digest, &span.range_sha256);
        let mut provenance = span.provenance.iter().collect::<Vec<_>>();
        provenance.sort();
        digest.update((provenance.len() as u64).to_le_bytes());
        for source in provenance {
            field(digest, &source.data_source_id);
            field(digest, &source.inventory_id);
            field(digest, &source.object_identity_sha256);
        }
    }
}

fn field(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value.as_bytes());
}
