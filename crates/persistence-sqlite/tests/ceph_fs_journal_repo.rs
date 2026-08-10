use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_fs_journal_repo::{
            cephfs_journal_input_sha256, cephfs_journal_map_provenance_sha256,
            cephfs_journal_projection_sha256, cephfs_journal_u64_hex, CephFsJournalEventRecord,
            CephFsJournalEventSpanRecord, CephFsJournalMapProvenanceRecord,
            CephFsJournalReplayManifest, CephFsJournalReplayProjection, CephFsJournalRepo,
            CephFsJournalRepoError, CephFsJournalWriteOutcome, CEPHFS_JOURNAL_DECODER_PROFILE,
            CEPHFS_JOURNAL_SCHEMA_VERSION,
        },
        ceph_fs_metadata_inventory_repo::{
            cephfs_metadata_inventory_sha256, CephFsMetadataInventory,
            CephFsMetadataInventoryManifest, CephFsMetadataInventoryRepo,
            CephFsMetadataObjectProjection, CEPHFS_METADATA_CLASSIFIER_PROFILE,
            CEPHFS_METADATA_SCHEMA_VERSION,
        },
    },
    runner,
};
use rusqlite::{params, Connection};

const INVENTORY: &str = "inventory-a";
const SOURCE: &str = "source-a";
const FILESYSTEM: &str = "ceph-fs:cluster-a:1:17:7";
const POINTER_LOCATOR: &str = "1:7:h:h3430302e3030303030303030:17";
const HEADER_LOCATOR: &str = "1:7:h:h3230302e3030303030303030:17";
const DATA_LOCATOR: &str = "1:7:h:h3230302e3030303030303031:17";

#[derive(Clone)]
struct FixtureObjects {
    pointer: String,
    header: String,
    data: String,
    metadata_inventory_sha256: String,
}

fn setup() -> (Connection, FixtureObjects) {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    seed_source_chain(&conn);
    let pointer = insert_object(&conn, '1', b"400.00000000");
    let header = insert_object(&conn, '2', b"200.00000000");
    let data = insert_object(&conn, '3', b"200.00000001");
    let inventory = metadata_inventory(&pointer, &header, &data);
    let metadata_inventory_sha256 = inventory.manifest.inventory_sha256.clone();
    CephFsMetadataInventoryRepo::new(&conn)
        .replace(&inventory)
        .expect("persist Stage 2 inventory");
    (
        conn,
        FixtureObjects {
            pointer,
            header,
            data,
            metadata_inventory_sha256,
        },
    )
}

fn seed_source_chain(conn: &Connection) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-19T00:00:00Z')",
        [SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (?1, ?2, ?1, 'block', 1048576, 1, 0, 'BlueStore OSD', 1,
                   1, 'singleReplica', 1, '{}')",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
                   4096, 1, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [INVENTORY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
                   'leveldb.BytewiseComparator', 100, 150, 142, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluestore_semantic_scans (
            inventory_id, schema_version, decode_profile, sharding_sha256,
            latest_state_sha256, semantic_sha256,
            s_latest_count, s_decoded_count, s_deferred_count,
            c_latest_count, c_decoded_count, c_deferred_count,
            o_latest_count, o_decoded_count, o_deferred_count,
            x_latest_count, x_decoded_count, x_deferred_count,
            collection_count, object_count, blob_count, onode_shard_count,
            logical_extent_count, physical_extent_count, checksum_chunk_count,
            shared_blob_count, shared_ref_extent_count, profile_complete
         ) VALUES (?1, 1, 'scox-v1', ?2, ?3, ?4,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                   0, 3, 0, 0, 0, 0, 0, 0, 0, 1)",
        params![INVENTORY, "a".repeat(64), "b".repeat(64), "c".repeat(64)],
    )
    .unwrap();
}

fn insert_object(conn: &Connection, identity: char, name: &[u8]) -> String {
    let object_identity = identity.to_string().repeat(64);
    conn.execute(
        "INSERT INTO ceph_bluestore_objects (
            inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
            decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
            object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
            flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
            flag_per_pg_omap, flags_unknown_bits, attribute_count,
            attribute_value_bytes, attributes_sha256, expected_object_size,
            expected_write_size, allocation_hint_flags, zone_ref_count,
            extent_storage, spanning_blob_version, declared_spanning_blob_count,
            decode_status, deferred_reason, onode_shard_count, blob_count,
            logical_extent_count, physical_extent_count
         ) VALUES (
            ?1, ?2, -1, 7, 1, 2147483648, X'', NULL, ?3,
            'fffffffffffffffe', '0000000000000000', 1, 1, 65536,
            0, 0, 0, 0, 0, 0, 0, 0, ?4, 65536, 65536, 0, 0,
            'inline', 0, 0, 'parsed', NULL, 0, 0, 0, 0
         )",
        params![INVENTORY, object_identity, name, "e".repeat(64)],
    )
    .unwrap();
    object_identity
}

fn metadata_inventory(pointer: &str, header: &str, data: &str) -> CephFsMetadataInventory {
    let mut inventory = CephFsMetadataInventory {
        manifest: CephFsMetadataInventoryManifest {
            filesystem_identity: FILESYSTEM.to_string(),
            inventory_id: INVENTORY.to_string(),
            data_source_id: SOURCE.to_string(),
            filesystem_id: 1,
            fsmap_epoch: 17,
            metadata_pool_id: 7,
            schema_version: CEPHFS_METADATA_SCHEMA_VERSION,
            classifier_profile: CEPHFS_METADATA_CLASSIFIER_PROFILE.to_string(),
            source_semantic_sha256: "c".repeat(64),
            inventory_sha256: String::new(),
            object_count: 3,
            unknown_object_count: 0,
            complete: true,
        },
        objects: vec![
            metadata_object(pointer, POINTER_LOCATOR, "journal_pointer", '4'),
            metadata_object(header, HEADER_LOCATOR, "journal_data", '5'),
            metadata_object(data, DATA_LOCATOR, "journal_data", '6'),
        ],
    };
    inventory.manifest.inventory_sha256 =
        cephfs_metadata_inventory_sha256(&inventory.manifest, &inventory.objects);
    inventory
}

fn metadata_object(
    object_identity: &str,
    locator: &str,
    classifier_rule: &str,
    digest: char,
) -> CephFsMetadataObjectProjection {
    CephFsMetadataObjectProjection {
        object_identity_sha256: object_identity.to_string(),
        locator: locator.to_string(),
        candidate_mask: 0,
        classification_state: "classified".to_string(),
        classifier_rule: classifier_rule.to_string(),
        record_sha256: digest.to_string().repeat(64),
    }
}

fn projection(objects: &FixtureObjects) -> CephFsJournalReplayProjection {
    let mut projection = CephFsJournalReplayProjection {
        manifest: CephFsJournalReplayManifest {
            filesystem_identity: FILESYSTEM.to_string(),
            inventory_id: INVENTORY.to_string(),
            data_source_id: SOURCE.to_string(),
            rank: 0,
            filesystem_id: 1,
            fsmap_epoch: 17,
            mdsmap_epoch: 23,
            rank_incarnation: 4,
            rank_gid_hex: cephfs_journal_u64_hex(u64::MAX),
            pointer_front_inode_hex: cephfs_journal_u64_hex(0x200),
            pointer_back_inode_hex: cephfs_journal_u64_hex(0),
            journal_inode_hex: cephfs_journal_u64_hex(0x200),
            schema_version: CEPHFS_JOURNAL_SCHEMA_VERSION,
            decoder_profile: CEPHFS_JOURNAL_DECODER_PROFILE.to_string(),
            source_semantic_sha256: "c".repeat(64),
            metadata_inventory_sha256: objects.metadata_inventory_sha256.clone(),
            raw_fsmap_snapshot_sha256: "a".repeat(64),
            raw_mdsmap_snapshot_sha256: "b".repeat(64),
            map_provenance_sha256: String::new(),
            map_provenance_count: 2,
            pointer_locator: POINTER_LOCATOR.to_string(),
            pointer_object_identity_sha256: objects.pointer.clone(),
            pointer_range_offset_hex: cephfs_journal_u64_hex(0),
            pointer_range_length_hex: cephfs_journal_u64_hex(16),
            pointer_range_sha256: "7".repeat(64),
            header_locator: HEADER_LOCATOR.to_string(),
            header_object_identity_sha256: objects.header.clone(),
            header_range_offset_hex: cephfs_journal_u64_hex(0),
            header_range_length_hex: cephfs_journal_u64_hex(64),
            header_range_sha256: "8".repeat(64),
            trimmed_pos_hex: cephfs_journal_u64_hex(0x800),
            expire_pos_hex: cephfs_journal_u64_hex(0x1000),
            unused_pos_hex: cephfs_journal_u64_hex(0),
            write_pos_hex: cephfs_journal_u64_hex(0x1024),
            committed_header_tail_hex: cephfs_journal_u64_hex(0x1024),
            framing_safe_pos_hex: cephfs_journal_u64_hex(0x1024),
            namespace_safe_pos_hex: Some(cephfs_journal_u64_hex(0x1024)),
            sequence_safe_pos_hex: cephfs_journal_u64_hex(0x1024),
            stream_format: "resilient".to_string(),
            framing_status: "complete_to_header_tail".to_string(),
            stop_reason: None,
            namespace_stop_reason: None,
            sequence_stop_reason: None,
            event_count: 1,
            input_sha256: String::new(),
            consensus_replay_sha256: "d".repeat(64),
            projection_sha256: String::new(),
        },
        map_provenance: ["monitor-a", "monitor-b"]
            .into_iter()
            .map(|source| CephFsJournalMapProvenanceRecord {
                filesystem_identity: FILESYSTEM.to_string(),
                inventory_id: INVENTORY.to_string(),
                rank: 0,
                source_identity: source.to_string(),
                source_inventory_identity: format!("map-17-{source}"),
                captured_at: "2026-07-19T10:00:00.000000000Z".to_string(),
                raw_fsmap_snapshot_sha256: "a".repeat(64),
                raw_mdsmap_snapshot_sha256: "b".repeat(64),
            })
            .collect(),
        events: vec![CephFsJournalEventRecord {
            filesystem_identity: FILESYSTEM.to_string(),
            inventory_id: INVENTORY.to_string(),
            rank: 0,
            event_ordinal: 0,
            segment_sequence_hex: Some(cephfs_journal_u64_hex(u64::MAX)),
            event_sequence_hex: Some(cephfs_journal_u64_hex(u64::MAX)),
            sequence_disposition: "resolved".to_string(),
            logical_offset_hex: cephfs_journal_u64_hex(0x1000),
            logical_end_hex: cephfs_journal_u64_hex(0x1024),
            payload_length: 16,
            payload_sha256: "9".repeat(64),
            event_type: 101,
            event_kind: "lid".to_string(),
            event_encoding: "versioned".to_string(),
            event_version: Some(1),
            event_compat_version: Some(1),
        }],
        spans: vec![CephFsJournalEventSpanRecord {
            filesystem_identity: FILESYSTEM.to_string(),
            inventory_id: INVENTORY.to_string(),
            rank: 0,
            event_ordinal: 0,
            span_ordinal: 0,
            object_locator: DATA_LOCATOR.to_string(),
            object_identity_sha256: objects.data.clone(),
            logical_offset_hex: cephfs_journal_u64_hex(0x1000),
            object_offset_hex: cephfs_journal_u64_hex(0),
            range_length_hex: cephfs_journal_u64_hex(0x24),
            range_sha256: "a".repeat(64),
        }],
    };
    seal(&mut projection);
    projection
}

fn seal(projection: &mut CephFsJournalReplayProjection) {
    projection.manifest.map_provenance_sha256 =
        cephfs_journal_map_provenance_sha256(&projection.map_provenance);
    projection.manifest.input_sha256 = cephfs_journal_input_sha256(&projection.manifest);
    projection.manifest.projection_sha256 = cephfs_journal_projection_sha256(
        &projection.manifest,
        &projection.events,
        &projection.spans,
    );
}

#[test]
fn complete_projection_round_trips_zero_unused_and_max_u64() {
    let (conn, objects) = setup();
    let projection = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);

    assert_eq!(
        repo.replace(&projection).unwrap(),
        CephFsJournalWriteOutcome::Replaced
    );
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(projection.clone())
    );
    assert_eq!(
        repo.replace(&projection).unwrap(),
        CephFsJournalWriteOutcome::Unchanged
    );
    let stored = repo.find(FILESYSTEM, INVENTORY, 0).unwrap().unwrap();
    assert_eq!(stored.manifest.unused_pos_hex, "0000000000000000");
    assert_eq!(stored.manifest.rank_gid_hex, "ffffffffffffffff");
    assert_eq!(
        stored.events[0].event_sequence_hex.as_deref(),
        Some("ffffffffffffffff")
    );

    let mut max_unused = projection;
    max_unused.manifest.unused_pos_hex = "ffffffffffffffff".to_string();
    seal(&mut max_unused);
    assert_eq!(
        repo.replace(&max_unused).unwrap(),
        CephFsJournalWriteOutcome::Replaced
    );
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(max_unused)
    );
}

#[test]
fn boundary_sequences_resolve_and_ordinary_events_increment_per_segment() {
    let (conn, objects) = setup();
    let mut projection = projection(&objects);
    let event_template = projection.events[0].clone();
    let span_template = projection.spans[0].clone();
    let segment_events = [(0x10, 0x10), (0x10, 0x11), (0x20, 0x20), (0x20, 0x21)];
    projection.events = segment_events
        .into_iter()
        .enumerate()
        .map(|(ordinal, (segment, sequence))| {
            let logical_offset = 0x1000 + ordinal as u64 * 0x24;
            let boundary = ordinal % 2 == 0;
            CephFsJournalEventRecord {
                event_ordinal: ordinal as u64,
                segment_sequence_hex: Some(cephfs_journal_u64_hex(segment)),
                event_sequence_hex: Some(cephfs_journal_u64_hex(sequence)),
                sequence_disposition: "resolved".to_string(),
                logical_offset_hex: cephfs_journal_u64_hex(logical_offset),
                logical_end_hex: cephfs_journal_u64_hex(logical_offset + 0x24),
                payload_sha256: format!("{:064x}", ordinal + 1),
                event_type: if boundary { 100 } else { 51 },
                event_kind: if boundary { "segment" } else { "noop" }.to_string(),
                ..event_template.clone()
            }
        })
        .collect();
    projection.spans = (0..4)
        .map(|ordinal| {
            let logical_offset = 0x1000 + ordinal * 0x24;
            CephFsJournalEventSpanRecord {
                event_ordinal: ordinal,
                logical_offset_hex: cephfs_journal_u64_hex(logical_offset),
                object_offset_hex: cephfs_journal_u64_hex(ordinal * 0x24),
                range_length_hex: cephfs_journal_u64_hex(0x24),
                range_sha256: format!("{:064x}", ordinal + 5),
                ..span_template.clone()
            }
        })
        .collect();
    projection.manifest.write_pos_hex = cephfs_journal_u64_hex(0x1090);
    projection.manifest.committed_header_tail_hex = cephfs_journal_u64_hex(0x1090);
    projection.manifest.framing_safe_pos_hex = cephfs_journal_u64_hex(0x1090);
    projection.manifest.sequence_safe_pos_hex = cephfs_journal_u64_hex(0x1090);
    projection.manifest.namespace_safe_pos_hex = Some(cephfs_journal_u64_hex(0x1090));
    projection.manifest.event_count = 4;
    seal(&mut projection);

    let repo = CephFsJournalRepo::new(&conn);
    assert_eq!(
        repo.replace(&projection).unwrap(),
        CephFsJournalWriteOutcome::Replaced
    );
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(projection)
    );
}

#[test]
fn explicit_semantic_unavailable_boundary_round_trips() {
    let (conn, objects) = setup();
    let mut projection = projection(&objects);
    projection.events[0].segment_sequence_hex = None;
    projection.events[0].event_sequence_hex = None;
    projection.events[0].sequence_disposition = "semantic_unavailable".to_string();
    projection.manifest.sequence_safe_pos_hex = cephfs_journal_u64_hex(0x1000);
    projection.manifest.sequence_stop_reason = Some("unsupported_semantics".to_string());
    seal(&mut projection);

    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&projection).unwrap();
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(projection)
    );
}

#[test]
fn only_the_first_lid_is_a_resolved_major_boundary() {
    let (conn, objects) = setup();
    let mut projection = projection(&objects);
    projection.events[0].event_type = 101;
    projection.events[0].event_kind = "lid".to_string();
    projection.events[0].segment_sequence_hex = Some(cephfs_journal_u64_hex(0x10));
    projection.events[0].event_sequence_hex = Some(cephfs_journal_u64_hex(0x10));
    let mut ignored = projection.events[0].clone();
    ignored.event_ordinal = 1;
    ignored.segment_sequence_hex = None;
    ignored.event_sequence_hex = None;
    ignored.sequence_disposition = "ignored_lid".to_string();
    ignored.logical_offset_hex = cephfs_journal_u64_hex(0x1024);
    ignored.logical_end_hex = cephfs_journal_u64_hex(0x1048);
    ignored.payload_sha256 = "e".repeat(64);
    projection.events.push(ignored);
    let mut ignored_span = projection.spans[0].clone();
    ignored_span.event_ordinal = 1;
    ignored_span.logical_offset_hex = cephfs_journal_u64_hex(0x1024);
    ignored_span.object_offset_hex = cephfs_journal_u64_hex(0x24);
    ignored_span.range_sha256 = "f".repeat(64);
    projection.spans.push(ignored_span);
    projection.manifest.write_pos_hex = cephfs_journal_u64_hex(0x1048);
    projection.manifest.committed_header_tail_hex = cephfs_journal_u64_hex(0x1048);
    projection.manifest.framing_safe_pos_hex = cephfs_journal_u64_hex(0x1048);
    projection.manifest.sequence_safe_pos_hex = cephfs_journal_u64_hex(0x1048);
    projection.manifest.namespace_safe_pos_hex = Some(cephfs_journal_u64_hex(0x1048));
    projection.manifest.event_count = 2;
    seal(&mut projection);

    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&projection).unwrap();
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(projection)
    );
}

#[test]
fn same_input_with_different_projection_is_a_determinism_conflict() {
    let (conn, objects) = setup();
    let original = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&original).unwrap();

    let mut conflicting = original.clone();
    conflicting.events[0].payload_sha256 = "d".repeat(64);
    conflicting.manifest.projection_sha256 = cephfs_journal_projection_sha256(
        &conflicting.manifest,
        &conflicting.events,
        &conflicting.spans,
    );
    assert!(matches!(
        repo.replace(&conflicting),
        Err(CephFsJournalRepoError::DeterminismConflict)
    ));
    assert_eq!(repo.find(FILESYSTEM, INVENTORY, 0).unwrap(), Some(original));
}

#[test]
fn invalid_object_binding_preserves_the_previous_atomic_projection() {
    let (conn, objects) = setup();
    let original = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&original).unwrap();

    let mut invalid = original.clone();
    invalid.manifest.pointer_range_sha256 = "e".repeat(64);
    invalid.spans[0].object_identity_sha256 = "f".repeat(64);
    seal(&mut invalid);
    assert!(matches!(
        repo.replace(&invalid),
        Err(CephFsJournalRepoError::ObjectBindingMismatch)
    ));
    assert_eq!(repo.find(FILESYSTEM, INVENTORY, 0).unwrap(), Some(original));
}

#[test]
fn provenance_insert_failure_rolls_back_to_the_previous_complete_projection() {
    let (conn, objects) = setup();
    let original = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&original).unwrap();

    let mut replacement = original.clone();
    replacement.manifest.mdsmap_epoch += 1;
    seal(&mut replacement);
    conn.execute_batch(
        "CREATE TEMP TRIGGER abort_cephfs_journal_provenance_insert
         BEFORE INSERT ON ceph_fs_journal_map_provenance
         BEGIN
             SELECT RAISE(ABORT, 'forced provenance insertion failure');
         END;",
    )
    .unwrap();

    assert!(repo.replace(&replacement).is_err());
    assert_eq!(repo.find(FILESYSTEM, INVENTORY, 0).unwrap(), Some(original));
}

#[test]
fn incomplete_projection_round_trips_and_parent_delete_cascades() {
    let (conn, objects) = setup();
    let mut incomplete = projection(&objects);
    incomplete.manifest.unused_pos_hex = cephfs_journal_u64_hex(0x1040);
    incomplete.manifest.write_pos_hex = cephfs_journal_u64_hex(0x1040);
    incomplete.manifest.committed_header_tail_hex = cephfs_journal_u64_hex(0x1040);
    incomplete.manifest.namespace_safe_pos_hex = None;
    incomplete.manifest.framing_status = "incomplete".to_string();
    incomplete.manifest.stop_reason = Some("truncated_frame".to_string());
    incomplete.manifest.namespace_stop_reason = Some("framing_incomplete".to_string());
    seal(&mut incomplete);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&incomplete).unwrap();
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(incomplete)
    );

    conn.execute(
        "DELETE FROM ceph_fs_metadata_inventories
         WHERE filesystem_identity = ?1 AND inventory_id = ?2",
        params![FILESYSTEM, INVENTORY],
    )
    .unwrap();
    assert!(repo.find(FILESYSTEM, INVENTORY, 0).unwrap().is_none());
    for table in [
        "ceph_fs_journal_map_provenance",
        "ceph_fs_journal_events",
        "ceph_fs_journal_event_spans",
    ] {
        let count: u64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} did not cascade");
    }
}

#[test]
fn source_019_is_latest_and_reapplication_is_idempotent() {
    let (conn, _) = setup();
    assert_eq!(
        runner::latest_source_version(),
        "source_033_timeline_case_id_index"
    );
    assert_eq!(runner::run_source_all(&conn).unwrap(), 0);
    for table in [
        "ceph_fs_journal_replays",
        "ceph_fs_journal_map_provenance",
        "ceph_fs_journal_events",
        "ceph_fs_journal_event_spans",
    ] {
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing {table}");
    }
}

#[test]
fn rank_limit_matches_ceph_max_mds() {
    let (_conn, objects) = setup();
    let mut projection = projection(&objects);
    projection.manifest.rank = 0x100;
    for event in &mut projection.events {
        event.rank = 0x100;
    }
    for provenance in &mut projection.map_provenance {
        provenance.rank = 0x100;
    }
    for span in &mut projection.spans {
        span.rank = 0x100;
    }
    seal(&mut projection);

    assert!(matches!(
        persistence_sqlite::repositories::ceph_fs_journal_repo::validate_cephfs_journal_projection(
            &projection
        ),
        Err(CephFsJournalRepoError::Invalid(_))
    ));
}

#[test]
fn highest_valid_ceph_rank_is_accepted() {
    let (_conn, objects) = setup();
    let mut projection = projection(&objects);
    projection.manifest.rank = 0xff;
    projection.manifest.pointer_front_inode_hex = cephfs_journal_u64_hex(0x2ff);
    projection.manifest.journal_inode_hex = cephfs_journal_u64_hex(0x2ff);
    for event in &mut projection.events {
        event.rank = 0xff;
    }
    for provenance in &mut projection.map_provenance {
        provenance.rank = 0xff;
    }
    for span in &mut projection.spans {
        span.rank = 0xff;
    }
    seal(&mut projection);

    persistence_sqlite::repositories::ceph_fs_journal_repo::validate_cephfs_journal_projection(
        &projection,
    )
    .unwrap();
}

#[test]
fn unchanged_replace_compares_and_repairs_the_full_projection() {
    let (conn, objects) = setup();
    let projection = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&projection).unwrap();
    conn.execute(
        "DELETE FROM ceph_fs_journal_event_spans
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = 0",
        params![FILESYSTEM, INVENTORY],
    )
    .unwrap();
    conn.execute(
        "DELETE FROM ceph_fs_journal_map_provenance
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = 0
           AND source_identity = 'monitor-b'",
        params![FILESYSTEM, INVENTORY],
    )
    .unwrap();

    assert_eq!(
        repo.replace(&projection).unwrap(),
        CephFsJournalWriteOutcome::Replaced
    );
    assert_eq!(
        repo.find(FILESYSTEM, INVENTORY, 0).unwrap(),
        Some(projection)
    );
}

#[test]
fn map_provenance_and_sequence_disposition_tampering_is_detected() {
    let (conn, objects) = setup();
    let projection = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&projection).unwrap();

    conn.execute(
        "UPDATE ceph_fs_journal_map_provenance
         SET source_identity = 'tampered-monitor'
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = 0
           AND source_identity = 'monitor-a'",
        params![FILESYSTEM, INVENTORY],
    )
    .unwrap();
    assert!(matches!(
        repo.find(FILESYSTEM, INVENTORY, 0),
        Err(CephFsJournalRepoError::Invalid(_))
    ));

    repo.replace(&projection).unwrap();
    conn.execute(
        "UPDATE ceph_fs_journal_events
         SET sequence_disposition = 'semantic_unavailable',
             segment_sequence_hex = NULL,
             event_sequence_hex = NULL
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = 0",
        params![FILESYSTEM, INVENTORY],
    )
    .unwrap();
    assert!(matches!(
        repo.find(FILESYSTEM, INVENTORY, 0),
        Err(CephFsJournalRepoError::Invalid(_))
    ));
}

#[test]
fn decoder_impossible_controls_frames_types_versions_and_sequences_are_rejected() {
    let (_conn, objects) = setup();
    let original = projection(&objects);
    let mut invalid = Vec::new();

    let mut control_offset = original.clone();
    control_offset.manifest.pointer_range_offset_hex = cephfs_journal_u64_hex(1);
    invalid.push(control_offset);
    let mut control_length = original.clone();
    control_length.manifest.header_range_length_hex = cephfs_journal_u64_hex(64 * 1024 + 1);
    invalid.push(control_length);
    let mut frame_length = original.clone();
    frame_length.events[0].logical_end_hex = cephfs_journal_u64_hex(0x1025);
    invalid.push(frame_length);
    let mut type_kind = original.clone();
    type_kind.events[0].event_kind = "noop".to_string();
    invalid.push(type_kind);
    let mut outer_version = original.clone();
    outer_version.events[0].event_compat_version = Some(2);
    invalid.push(outer_version);
    let mut unresolved = original;
    unresolved.events[0].event_sequence_hex = None;
    invalid.push(unresolved);
    let mut impossible_unavailable = projection(&objects);
    impossible_unavailable.events[0].event_type = 51;
    impossible_unavailable.events[0].event_kind = "noop".to_string();
    impossible_unavailable.events[0].segment_sequence_hex = None;
    impossible_unavailable.events[0].event_sequence_hex = None;
    impossible_unavailable.events[0].sequence_disposition = "semantic_unavailable".to_string();
    impossible_unavailable.manifest.sequence_safe_pos_hex = cephfs_journal_u64_hex(0x1000);
    impossible_unavailable.manifest.sequence_stop_reason =
        Some("unsupported_semantics".to_string());
    invalid.push(impossible_unavailable);

    for projection in &mut invalid {
        seal(projection);
        assert!(matches!(
            persistence_sqlite::repositories::ceph_fs_journal_repo::validate_cephfs_journal_projection(
                projection
            ),
            Err(CephFsJournalRepoError::Invalid(_))
        ));
    }
}

#[test]
fn replay_data_source_is_enforced_by_a_composite_foreign_key() {
    let (conn, objects) = setup();
    let projection = projection(&objects);
    CephFsJournalRepo::new(&conn).replace(&projection).unwrap();
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES ('source-b', 'case-1', 'source-b', 'e01', 'source-b',
                   '2026-07-19T00:00:00Z')",
        [],
    )
    .unwrap();

    assert!(conn
        .execute(
            "UPDATE ceph_fs_journal_replays SET data_source_id = 'source-b'
             WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND rank = 0",
            params![FILESYSTEM, INVENTORY],
        )
        .is_err());
}

#[test]
fn deleting_an_event_object_removes_the_whole_replay_projection() {
    let (conn, objects) = setup();
    let projection = projection(&objects);
    let repo = CephFsJournalRepo::new(&conn);
    repo.replace(&projection).unwrap();

    conn.execute(
        "DELETE FROM ceph_fs_metadata_objects
         WHERE filesystem_identity = ?1 AND inventory_id = ?2
           AND object_identity_sha256 = ?3",
        params![FILESYSTEM, INVENTORY, objects.data],
    )
    .unwrap();

    assert!(repo.find(FILESYSTEM, INVENTORY, 0).unwrap().is_none());
    for table in [
        "ceph_fs_journal_map_provenance",
        "ceph_fs_journal_events",
        "ceph_fs_journal_event_spans",
    ] {
        let count: u64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} retained a partial replay");
    }
}
