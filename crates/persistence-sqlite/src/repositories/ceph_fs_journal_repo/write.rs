use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbError;

use super::{
    CephFsJournalEventRecord, CephFsJournalEventSpanRecord, CephFsJournalReplayManifest,
    CephFsJournalReplayProjection, CephFsJournalRepoError, CephFsJournalRepoResult,
    CephFsJournalWriteOutcome,
};

pub(super) fn replace(
    conn: &Connection,
    projection: &CephFsJournalReplayProjection,
) -> CephFsJournalRepoResult<CephFsJournalWriteOutcome> {
    let transaction = conn.unchecked_transaction().map_err(DbError::from)?;
    validate_source_binding(&transaction, &projection.manifest)?;
    validate_object_bindings(&transaction, projection)?;
    if let Some(outcome) = unchanged_or_conflicting(&transaction, projection)? {
        transaction.commit().map_err(DbError::from)?;
        return Ok(outcome);
    }
    delete_existing(&transaction, &projection.manifest)?;
    insert_manifest(&transaction, &projection.manifest)?;
    insert_map_provenance(&transaction, &projection.map_provenance)?;
    insert_events(&transaction, &projection.events)?;
    insert_spans(&transaction, &projection.spans)?;
    transaction.commit().map_err(DbError::from)?;
    Ok(CephFsJournalWriteOutcome::Replaced)
}

fn validate_source_binding(
    conn: &Connection,
    manifest: &CephFsJournalReplayManifest,
) -> CephFsJournalRepoResult<()> {
    let binding = conn
        .query_row(
            "SELECT data_source_id, filesystem_id, fsmap_epoch,
                    source_semantic_sha256, inventory_sha256
             FROM ceph_fs_metadata_inventories
             WHERE filesystem_identity = ?1 AND inventory_id = ?2",
            params![manifest.filesystem_identity, manifest.inventory_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(DbError::from)?;
    let matches = binding.is_some_and(
        |(data_source_id, filesystem_id, fsmap_epoch, semantic_sha256, inventory_sha256)| {
            data_source_id == manifest.data_source_id
                && filesystem_id == manifest.filesystem_id
                && fsmap_epoch == manifest.fsmap_epoch
                && semantic_sha256 == manifest.source_semantic_sha256
                && inventory_sha256 == manifest.metadata_inventory_sha256
        },
    );
    if !matches {
        return Err(CephFsJournalRepoError::SourceBindingMismatch);
    }
    Ok(())
}

fn validate_object_bindings(
    conn: &Connection,
    projection: &CephFsJournalReplayProjection,
) -> CephFsJournalRepoResult<()> {
    let manifest = &projection.manifest;
    let mut bindings = HashSet::new();
    bindings.insert((
        manifest.pointer_object_identity_sha256.as_str(),
        manifest.pointer_locator.as_str(),
    ));
    bindings.insert((
        manifest.header_object_identity_sha256.as_str(),
        manifest.header_locator.as_str(),
    ));
    for span in &projection.spans {
        bindings.insert((
            span.object_identity_sha256.as_str(),
            span.object_locator.as_str(),
        ));
    }
    let mut statement = conn
        .prepare_cached(
            "SELECT EXISTS(
                SELECT 1 FROM ceph_fs_metadata_objects
                WHERE filesystem_identity = ?1
                  AND inventory_id = ?2
                  AND object_identity_sha256 = ?3
                  AND locator = ?4
             )",
        )
        .map_err(DbError::from)?;
    for (object_identity, locator) in bindings {
        let exists: bool = statement
            .query_row(
                params![
                    manifest.filesystem_identity,
                    manifest.inventory_id,
                    object_identity,
                    locator
                ],
                |row| row.get(0),
            )
            .map_err(DbError::from)?;
        if !exists {
            return Err(CephFsJournalRepoError::ObjectBindingMismatch);
        }
    }
    Ok(())
}

fn unchanged_or_conflicting(
    conn: &Connection,
    projection: &CephFsJournalReplayProjection,
) -> CephFsJournalRepoResult<Option<CephFsJournalWriteOutcome>> {
    let manifest = &projection.manifest;
    let existing = conn
        .query_row(
            "SELECT input_sha256, projection_sha256
             FROM ceph_fs_journal_replays
             WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3",
            params![
                manifest.filesystem_identity,
                manifest.inventory_id,
                manifest.rank
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(DbError::from)?;
    let Some((input_sha256, projection_sha256)) = existing else {
        return Ok(None);
    };
    if input_sha256 != manifest.input_sha256 {
        return Ok(None);
    }
    if projection_sha256 != manifest.projection_sha256 {
        return Err(CephFsJournalRepoError::DeterminismConflict);
    }
    let stored = super::query::find(
        conn,
        &manifest.filesystem_identity,
        &manifest.inventory_id,
        manifest.rank,
    )?;
    Ok((stored.as_ref() == Some(projection)).then_some(CephFsJournalWriteOutcome::Unchanged))
}

fn delete_existing(
    conn: &Connection,
    manifest: &CephFsJournalReplayManifest,
) -> CephFsJournalRepoResult<()> {
    conn.execute(
        "DELETE FROM ceph_fs_journal_replays
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = ?3",
        params![
            manifest.filesystem_identity,
            manifest.inventory_id,
            manifest.rank
        ],
    )
    .map_err(DbError::from)?;
    Ok(())
}

fn insert_manifest(
    conn: &Connection,
    row: &CephFsJournalReplayManifest,
) -> CephFsJournalRepoResult<()> {
    conn.execute(
        "INSERT INTO ceph_fs_journal_replays (
            filesystem_identity, inventory_id, data_source_id, rank, filesystem_id,
            fsmap_epoch, mdsmap_epoch, rank_incarnation, rank_gid_hex,
            pointer_front_inode_hex, pointer_back_inode_hex, journal_inode_hex,
            schema_version, decoder_profile, source_semantic_sha256,
            metadata_inventory_sha256, raw_fsmap_snapshot_sha256,
            raw_mdsmap_snapshot_sha256, map_provenance_sha256,
            map_provenance_count, pointer_locator,
            pointer_object_identity_sha256, pointer_range_offset_hex,
            pointer_range_length_hex, pointer_range_sha256, header_locator,
            header_object_identity_sha256, header_range_offset_hex,
            header_range_length_hex, header_range_sha256, trimmed_pos_hex,
            expire_pos_hex, unused_pos_hex, write_pos_hex,
            committed_header_tail_hex, framing_safe_pos_hex,
            namespace_safe_pos_hex, sequence_safe_pos_hex, stream_format,
            framing_status, stop_reason, namespace_stop_reason,
            sequence_stop_reason, event_count, input_sha256,
            consensus_replay_sha256, projection_sha256
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40,
            ?41, ?42, ?43, ?44, ?45, ?46, ?47
         )",
        params![
            row.filesystem_identity,
            row.inventory_id,
            row.data_source_id,
            row.rank,
            row.filesystem_id,
            row.fsmap_epoch,
            row.mdsmap_epoch,
            row.rank_incarnation,
            row.rank_gid_hex,
            row.pointer_front_inode_hex,
            row.pointer_back_inode_hex,
            row.journal_inode_hex,
            row.schema_version,
            row.decoder_profile,
            row.source_semantic_sha256,
            row.metadata_inventory_sha256,
            row.raw_fsmap_snapshot_sha256,
            row.raw_mdsmap_snapshot_sha256,
            row.map_provenance_sha256,
            row.map_provenance_count,
            row.pointer_locator,
            row.pointer_object_identity_sha256,
            row.pointer_range_offset_hex,
            row.pointer_range_length_hex,
            row.pointer_range_sha256,
            row.header_locator,
            row.header_object_identity_sha256,
            row.header_range_offset_hex,
            row.header_range_length_hex,
            row.header_range_sha256,
            row.trimmed_pos_hex,
            row.expire_pos_hex,
            row.unused_pos_hex,
            row.write_pos_hex,
            row.committed_header_tail_hex,
            row.framing_safe_pos_hex,
            row.namespace_safe_pos_hex,
            row.sequence_safe_pos_hex,
            row.stream_format,
            row.framing_status,
            row.stop_reason,
            row.namespace_stop_reason,
            row.sequence_stop_reason,
            row.event_count,
            row.input_sha256,
            row.consensus_replay_sha256,
            row.projection_sha256,
        ],
    )
    .map_err(DbError::from)?;
    Ok(())
}

fn insert_map_provenance(
    conn: &Connection,
    records: &[super::CephFsJournalMapProvenanceRecord],
) -> CephFsJournalRepoResult<()> {
    let mut statement = conn
        .prepare_cached(
            "INSERT INTO ceph_fs_journal_map_provenance (
                filesystem_identity, inventory_id, rank, source_identity,
                source_inventory_identity, captured_at,
                raw_fsmap_snapshot_sha256, raw_mdsmap_snapshot_sha256
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(DbError::from)?;
    for row in records {
        statement
            .execute(params![
                row.filesystem_identity,
                row.inventory_id,
                row.rank,
                row.source_identity,
                row.source_inventory_identity,
                row.captured_at,
                row.raw_fsmap_snapshot_sha256,
                row.raw_mdsmap_snapshot_sha256,
            ])
            .map_err(DbError::from)?;
    }
    Ok(())
}

fn insert_events(
    conn: &Connection,
    events: &[CephFsJournalEventRecord],
) -> CephFsJournalRepoResult<()> {
    let mut statement = conn
        .prepare_cached(
            "INSERT INTO ceph_fs_journal_events (
                filesystem_identity, inventory_id, rank, event_ordinal,
                segment_sequence_hex, event_sequence_hex, sequence_disposition,
                logical_offset_hex, logical_end_hex, payload_length, payload_sha256,
                event_type, event_kind, event_encoding, event_version,
                event_compat_version
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16
             )",
        )
        .map_err(DbError::from)?;
    for row in events {
        statement
            .execute(params![
                row.filesystem_identity,
                row.inventory_id,
                row.rank,
                row.event_ordinal,
                row.segment_sequence_hex,
                row.event_sequence_hex,
                row.sequence_disposition,
                row.logical_offset_hex,
                row.logical_end_hex,
                row.payload_length,
                row.payload_sha256,
                row.event_type,
                row.event_kind,
                row.event_encoding,
                row.event_version,
                row.event_compat_version,
            ])
            .map_err(DbError::from)?;
    }
    Ok(())
}

fn insert_spans(
    conn: &Connection,
    spans: &[CephFsJournalEventSpanRecord],
) -> CephFsJournalRepoResult<()> {
    let mut statement = conn
        .prepare_cached(
            "INSERT INTO ceph_fs_journal_event_spans (
                filesystem_identity, inventory_id, rank, event_ordinal,
                span_ordinal, object_locator, object_identity_sha256,
                logical_offset_hex, object_offset_hex, range_length_hex,
                range_sha256
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .map_err(DbError::from)?;
    for row in spans {
        statement
            .execute(params![
                row.filesystem_identity,
                row.inventory_id,
                row.rank,
                row.event_ordinal,
                row.span_ordinal,
                row.object_locator,
                row.object_identity_sha256,
                row.logical_offset_hex,
                row.object_offset_hex,
                row.range_length_hex,
                row.range_sha256,
            ])
            .map_err(DbError::from)?;
    }
    Ok(())
}
