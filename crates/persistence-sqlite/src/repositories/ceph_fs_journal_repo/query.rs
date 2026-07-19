use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbError;

use super::{
    CephFsJournalEventRecord, CephFsJournalEventSpanRecord, CephFsJournalMapProvenanceRecord,
    CephFsJournalReplayManifest, CephFsJournalReplayProjection, CephFsJournalRepoResult,
};

pub(super) fn find(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    rank: u32,
) -> CephFsJournalRepoResult<Option<CephFsJournalReplayProjection>> {
    let Some(manifest) = find_manifest(conn, filesystem_identity, inventory_id, rank)? else {
        return Ok(None);
    };
    let map_provenance = find_map_provenance(conn, filesystem_identity, inventory_id, rank)?;
    let events = find_events(conn, filesystem_identity, inventory_id, rank)?;
    let spans = find_spans(conn, filesystem_identity, inventory_id, rank)?;
    Ok(Some(CephFsJournalReplayProjection {
        manifest,
        map_provenance,
        events,
        spans,
    }))
}

fn find_manifest(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    rank: u32,
) -> CephFsJournalRepoResult<Option<CephFsJournalReplayManifest>> {
    conn.query_row(
        "SELECT filesystem_identity, inventory_id, data_source_id, rank,
                filesystem_id, fsmap_epoch, mdsmap_epoch, rank_incarnation,
                rank_gid_hex, pointer_front_inode_hex, pointer_back_inode_hex,
                journal_inode_hex, schema_version, decoder_profile,
                source_semantic_sha256, metadata_inventory_sha256,
                raw_fsmap_snapshot_sha256, raw_mdsmap_snapshot_sha256,
                map_provenance_sha256, map_provenance_count,
                pointer_locator, pointer_object_identity_sha256,
                pointer_range_offset_hex, pointer_range_length_hex,
                pointer_range_sha256, header_locator,
                header_object_identity_sha256, header_range_offset_hex,
                header_range_length_hex, header_range_sha256, trimmed_pos_hex,
                expire_pos_hex, unused_pos_hex, write_pos_hex,
                committed_header_tail_hex, framing_safe_pos_hex,
                namespace_safe_pos_hex, sequence_safe_pos_hex,
                stream_format, framing_status, stop_reason,
                namespace_stop_reason, sequence_stop_reason, event_count,
                input_sha256, consensus_replay_sha256, projection_sha256
         FROM ceph_fs_journal_replays
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3",
        params![filesystem_identity, inventory_id, rank],
        map_manifest,
    )
    .optional()
    .map_err(DbError::from)
    .map_err(Into::into)
}

fn find_map_provenance(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    rank: u32,
) -> CephFsJournalRepoResult<Vec<CephFsJournalMapProvenanceRecord>> {
    let mut statement = conn
        .prepare(
            "SELECT filesystem_identity, inventory_id, rank, source_identity,
                    source_inventory_identity, captured_at,
                    raw_fsmap_snapshot_sha256, raw_mdsmap_snapshot_sha256
             FROM ceph_fs_journal_map_provenance
             WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3
             ORDER BY source_identity, source_inventory_identity, captured_at",
        )
        .map_err(DbError::from)?;
    let rows = statement
        .query_map(params![filesystem_identity, inventory_id, rank], |row| {
            Ok(CephFsJournalMapProvenanceRecord {
                filesystem_identity: row.get(0)?,
                inventory_id: row.get(1)?,
                rank: row.get(2)?,
                source_identity: row.get(3)?,
                source_inventory_identity: row.get(4)?,
                captured_at: row.get(5)?,
                raw_fsmap_snapshot_sha256: row.get(6)?,
                raw_mdsmap_snapshot_sha256: row.get(7)?,
            })
        })
        .map_err(DbError::from)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DbError::from)
        .map_err(Into::into)
}

fn find_events(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    rank: u32,
) -> CephFsJournalRepoResult<Vec<CephFsJournalEventRecord>> {
    let mut statement = conn
        .prepare(
            "SELECT filesystem_identity, inventory_id, rank, event_ordinal,
                    segment_sequence_hex, event_sequence_hex, sequence_disposition,
                    logical_offset_hex, logical_end_hex, payload_length,
                    payload_sha256, event_type,
                    event_kind, event_encoding, event_version, event_compat_version
             FROM ceph_fs_journal_events
             WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3
             ORDER BY event_ordinal",
        )
        .map_err(DbError::from)?;
    let rows = statement
        .query_map(params![filesystem_identity, inventory_id, rank], map_event)
        .map_err(DbError::from)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DbError::from)
        .map_err(Into::into)
}

fn find_spans(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    rank: u32,
) -> CephFsJournalRepoResult<Vec<CephFsJournalEventSpanRecord>> {
    let mut statement = conn
        .prepare(
            "SELECT filesystem_identity, inventory_id, rank, event_ordinal,
                    span_ordinal, object_locator, object_identity_sha256,
                    logical_offset_hex, object_offset_hex, range_length_hex,
                    range_sha256
             FROM ceph_fs_journal_event_spans
             WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3
             ORDER BY event_ordinal, span_ordinal",
        )
        .map_err(DbError::from)?;
    let rows = statement
        .query_map(params![filesystem_identity, inventory_id, rank], map_span)
        .map_err(DbError::from)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DbError::from)
        .map_err(Into::into)
}

fn map_manifest(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsJournalReplayManifest> {
    Ok(CephFsJournalReplayManifest {
        filesystem_identity: row.get(0)?,
        inventory_id: row.get(1)?,
        data_source_id: row.get(2)?,
        rank: row.get(3)?,
        filesystem_id: row.get(4)?,
        fsmap_epoch: row.get(5)?,
        mdsmap_epoch: row.get(6)?,
        rank_incarnation: row.get(7)?,
        rank_gid_hex: row.get(8)?,
        pointer_front_inode_hex: row.get(9)?,
        pointer_back_inode_hex: row.get(10)?,
        journal_inode_hex: row.get(11)?,
        schema_version: row.get(12)?,
        decoder_profile: row.get(13)?,
        source_semantic_sha256: row.get(14)?,
        metadata_inventory_sha256: row.get(15)?,
        raw_fsmap_snapshot_sha256: row.get(16)?,
        raw_mdsmap_snapshot_sha256: row.get(17)?,
        map_provenance_sha256: row.get(18)?,
        map_provenance_count: row.get(19)?,
        pointer_locator: row.get(20)?,
        pointer_object_identity_sha256: row.get(21)?,
        pointer_range_offset_hex: row.get(22)?,
        pointer_range_length_hex: row.get(23)?,
        pointer_range_sha256: row.get(24)?,
        header_locator: row.get(25)?,
        header_object_identity_sha256: row.get(26)?,
        header_range_offset_hex: row.get(27)?,
        header_range_length_hex: row.get(28)?,
        header_range_sha256: row.get(29)?,
        trimmed_pos_hex: row.get(30)?,
        expire_pos_hex: row.get(31)?,
        unused_pos_hex: row.get(32)?,
        write_pos_hex: row.get(33)?,
        committed_header_tail_hex: row.get(34)?,
        framing_safe_pos_hex: row.get(35)?,
        namespace_safe_pos_hex: row.get(36)?,
        sequence_safe_pos_hex: row.get(37)?,
        stream_format: row.get(38)?,
        framing_status: row.get(39)?,
        stop_reason: row.get(40)?,
        namespace_stop_reason: row.get(41)?,
        sequence_stop_reason: row.get(42)?,
        event_count: row.get(43)?,
        input_sha256: row.get(44)?,
        consensus_replay_sha256: row.get(45)?,
        projection_sha256: row.get(46)?,
    })
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsJournalEventRecord> {
    Ok(CephFsJournalEventRecord {
        filesystem_identity: row.get(0)?,
        inventory_id: row.get(1)?,
        rank: row.get(2)?,
        event_ordinal: row.get(3)?,
        segment_sequence_hex: row.get(4)?,
        event_sequence_hex: row.get(5)?,
        sequence_disposition: row.get(6)?,
        logical_offset_hex: row.get(7)?,
        logical_end_hex: row.get(8)?,
        payload_length: row.get(9)?,
        payload_sha256: row.get(10)?,
        event_type: row.get(11)?,
        event_kind: row.get(12)?,
        event_encoding: row.get(13)?,
        event_version: row.get(14)?,
        event_compat_version: row.get(15)?,
    })
}

fn map_span(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsJournalEventSpanRecord> {
    Ok(CephFsJournalEventSpanRecord {
        filesystem_identity: row.get(0)?,
        inventory_id: row.get(1)?,
        rank: row.get(2)?,
        event_ordinal: row.get(3)?,
        span_ordinal: row.get(4)?,
        object_locator: row.get(5)?,
        object_identity_sha256: row.get(6)?,
        logical_offset_hex: row.get(7)?,
        object_offset_hex: row.get(8)?,
        range_length_hex: row.get(9)?,
        range_sha256: row.get(10)?,
    })
}
