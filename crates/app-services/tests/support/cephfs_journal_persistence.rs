use std::collections::BTreeMap;

use app_services::ceph_reconstruction::{
    replay_cephfs_journal, CephFsDescriptor, CephFsDescriptorState, CephFsJournalReplay,
    CephFsJournalReplayLimits, CephFsObjectLocator, CephFsObjectMetadata, CephFsObjectRange,
    CephFsObjectRangeReader, CephFsObjectReadError, CephFsObjectReadProvenance, CephFsPoolBinding,
    CephFsPoolRole, CephFsRankBinding,
};
use ceph_wire::{
    format_cephfs_journal_data_object_name, format_cephfs_journal_pointer_object_name,
    CephFsJournalLayout, CephMdsDaemon, CephMdsState, CEPHFS_JOURNAL_MAGIC,
};
use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_fs_metadata_inventory_repo::{
        cephfs_metadata_inventory_sha256, CephFsMetadataInventory, CephFsMetadataInventoryManifest,
        CephFsMetadataInventoryRepo, CephFsMetadataObjectProjection,
        CEPHFS_METADATA_CLASSIFIER_PROFILE, CEPHFS_METADATA_SCHEMA_VERSION,
    },
    runner,
};
use rusqlite::{params, Connection};

pub const FILESYSTEM: &str = "ceph-fs:cluster-a:1:17:7";
const POOL_ID: i64 = 7;
const RANK: u32 = 0;
const FRONT_INODE: u64 = 0x200;
const SENTINEL: u64 = 0x3141_5926_5358_9793;

#[derive(Debug, Clone)]
pub struct SourceObjects {
    pub source_id: String,
    pub inventory_id: String,
    pub pointer: String,
    pub header: String,
    pub data: String,
}

struct FixtureObject {
    bytes: Vec<u8>,
    provenance: Vec<CephFsObjectReadProvenance>,
}

struct FixtureReader {
    descriptor: CephFsDescriptor,
    objects: BTreeMap<String, FixtureObject>,
}

impl CephFsObjectRangeReader for FixtureReader {
    fn inspect_object(
        &mut self,
        locator: &CephFsObjectLocator,
    ) -> Result<CephFsObjectMetadata, CephFsObjectReadError> {
        let canonical = locator.canonical();
        let object =
            self.objects
                .get(&canonical)
                .ok_or_else(|| CephFsObjectReadError::ObjectNotFound {
                    locator: canonical.clone(),
                })?;
        Ok(CephFsObjectMetadata {
            filesystem_identity: self.descriptor.identity.clone(),
            locator: canonical,
            object_size: object.bytes.len() as u64,
            provenance: object.provenance.clone(),
        })
    }

    fn read_range(
        &mut self,
        locator: &CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError> {
        let canonical = locator.canonical();
        let object =
            self.objects
                .get(&canonical)
                .ok_or_else(|| CephFsObjectReadError::ObjectNotFound {
                    locator: canonical.clone(),
                })?;
        let end = offset.checked_add(length as u64).ok_or_else(|| {
            CephFsObjectReadError::RangeOverflow {
                locator: canonical.clone(),
            }
        })?;
        if end > object.bytes.len() as u64 {
            return Err(CephFsObjectReadError::RangeOutOfBounds {
                locator: canonical,
                object_size: object.bytes.len() as u64,
            });
        }
        Ok(CephFsObjectRange {
            filesystem_identity: self.descriptor.identity.clone(),
            locator: locator.canonical(),
            object_size: object.bytes.len() as u64,
            offset,
            bytes: object.bytes[offset as usize..end as usize].to_vec(),
            provenance: object.provenance.clone(),
        })
    }
}

pub fn setup_source(
    source_id: &str,
    inventory_id: &str,
    marker: char,
) -> (Connection, SourceObjects) {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    seed_source_chain(&conn, source_id, inventory_id);
    let objects = SourceObjects {
        source_id: source_id.to_string(),
        inventory_id: inventory_id.to_string(),
        pointer: marker.to_string().repeat(64),
        header: char::from_u32(marker as u32 + 1)
            .expect("next marker")
            .to_string()
            .repeat(64),
        data: char::from_u32(marker as u32 + 2)
            .expect("next marker")
            .to_string()
            .repeat(64),
    };
    insert_object(&conn, inventory_id, &objects.pointer, b"400.00000000");
    insert_object(&conn, inventory_id, &objects.header, b"200.00000000");
    insert_object(&conn, inventory_id, &objects.data, b"200.00000001");
    persist_metadata_inventory(&conn, &objects);
    (conn, objects)
}

pub fn replay_fixture(
    sources: &[SourceObjects],
    duplicate_first_data: bool,
) -> CephFsJournalReplay {
    replay_fixture_from_payloads(sources, duplicate_first_data, &[lid_event(1)])
}

pub fn replay_fixture_with_non_initial_lid(sources: &[SourceObjects]) -> CephFsJournalReplay {
    replay_fixture_from_payloads(sources, false, &[subtree_event_v6(10), lid_event(999)])
}

pub fn replay_fixture_with_empty_payload(sources: &[SourceObjects]) -> CephFsJournalReplay {
    replay_fixture_from_payloads(sources, false, &[Vec::new()])
}

fn replay_fixture_from_payloads(
    sources: &[SourceObjects],
    duplicate_first_data: bool,
    payloads: &[Vec<u8>],
) -> CephFsJournalReplay {
    let descriptor = descriptor();
    let layout = layout();
    let period = layout.period().expect("journal period");
    let mut position = period;
    let mut journal = Vec::new();
    for payload in payloads {
        let frame = resilient_frame(payload, position);
        position += frame.len() as u64;
        journal.extend_from_slice(&frame);
    }
    let write_pos = position;
    let mut reader = FixtureReader {
        descriptor: descriptor.clone(),
        objects: BTreeMap::new(),
    };
    insert_fixture_object(
        &mut reader,
        format_cephfs_journal_pointer_object_name(RANK).expect("pointer name"),
        pointer(),
        provenance(sources, |source| &source.pointer, false),
    );
    insert_fixture_object(
        &mut reader,
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 0).expect("header name"),
        header(layout, period, write_pos),
        provenance(sources, |source| &source.header, false),
    );
    insert_fixture_object(
        &mut reader,
        format_cephfs_journal_data_object_name(RANK, FRONT_INODE, 1).expect("data name"),
        journal,
        provenance(sources, |source| &source.data, duplicate_first_data),
    );
    replay_cephfs_journal(
        &descriptor,
        RANK,
        &mut reader,
        CephFsJournalReplayLimits::default(),
    )
    .expect("replay fixture journal")
}

fn provenance(
    sources: &[SourceObjects],
    object_identity: impl Fn(&SourceObjects) -> &String,
    duplicate_first: bool,
) -> Vec<CephFsObjectReadProvenance> {
    let mut provenance = sources
        .iter()
        .map(|source| CephFsObjectReadProvenance {
            data_source_id: source.source_id.clone(),
            inventory_id: source.inventory_id.clone(),
            object_identity_sha256: object_identity(source).clone(),
        })
        .collect::<Vec<_>>();
    if duplicate_first {
        provenance.push(provenance[0].clone());
    }
    provenance
}

fn insert_fixture_object(
    reader: &mut FixtureReader,
    object_name: String,
    bytes: Vec<u8>,
    provenance: Vec<CephFsObjectReadProvenance>,
) {
    let canonical = locator(object_name).canonical();
    reader
        .objects
        .insert(canonical, FixtureObject { bytes, provenance });
}

fn persist_metadata_inventory(conn: &Connection, source: &SourceObjects) {
    let objects = vec![
        metadata_object(&source.pointer, "400.00000000", "journal_pointer", '7'),
        metadata_object(&source.header, "200.00000000", "journal_data", '8'),
        metadata_object(&source.data, "200.00000001", "journal_data", '9'),
    ];
    let mut manifest = CephFsMetadataInventoryManifest {
        filesystem_identity: FILESYSTEM.to_string(),
        inventory_id: source.inventory_id.clone(),
        data_source_id: source.source_id.clone(),
        filesystem_id: 1,
        fsmap_epoch: 17,
        metadata_pool_id: POOL_ID,
        schema_version: CEPHFS_METADATA_SCHEMA_VERSION,
        classifier_profile: CEPHFS_METADATA_CLASSIFIER_PROFILE.to_string(),
        source_semantic_sha256: "c".repeat(64),
        inventory_sha256: String::new(),
        object_count: objects.len() as u64,
        unknown_object_count: 0,
        complete: true,
    };
    manifest.inventory_sha256 = cephfs_metadata_inventory_sha256(&manifest, &objects);
    CephFsMetadataInventoryRepo::new(conn)
        .replace(&CephFsMetadataInventory { manifest, objects })
        .expect("persist metadata inventory");
}

fn metadata_object(
    object_identity_sha256: &str,
    object_name: &str,
    classifier_rule: &str,
    digest: char,
) -> CephFsMetadataObjectProjection {
    CephFsMetadataObjectProjection {
        object_identity_sha256: object_identity_sha256.to_string(),
        locator: locator(object_name.to_string()).canonical(),
        candidate_mask: 0,
        classification_state: "classified".to_string(),
        classifier_rule: classifier_rule.to_string(),
        record_sha256: digest.to_string().repeat(64),
    }
}

fn insert_object(conn: &Connection, inventory_id: &str, identity: &str, name: &[u8]) {
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
        params![inventory_id, identity, name, "e".repeat(64)],
    )
    .expect("insert BlueStore object");
}

fn seed_source_chain(conn: &Connection, source_id: &str, inventory_id: &str) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-19T00:00:00Z')",
        [source_id],
    )
    .expect("insert data source");
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (?1, ?2, ?1, 'block', 1048576, 1, 0, 'BlueStore OSD', 1,
                   1, 'singleReplica', 1, '{}')",
        params![inventory_id, source_id],
    )
    .expect("insert OSD inventory");
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
                   4096, 1, 0, 0)",
        params![inventory_id, source_id],
    )
    .expect("insert BlueFS superblock");
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [inventory_id],
    )
    .expect("insert BlueFS replay");
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
                   'leveldb.BytewiseComparator', 100, 150, 142, 0, 0)",
        params![inventory_id, source_id],
    )
    .expect("insert RocksDB manifest");
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
        params![inventory_id, "a".repeat(64), "b".repeat(64), "c".repeat(64)],
    )
    .expect("insert semantic scan");
}

fn descriptor() -> CephFsDescriptor {
    CephFsDescriptor {
        identity: FILESYSTEM.to_string(),
        cluster_identity: "cluster-a".to_string(),
        filesystem_id: 1,
        name: "cephfs-a".to_string(),
        fsmap_epoch: 17,
        mdsmap_epoch: 23,
        state: CephFsDescriptorState::Present,
        metadata_pool: CephFsPoolBinding {
            pool_id: POOL_ID,
            role: CephFsPoolRole::Metadata,
            provenance: Vec::new(),
        },
        data_pools: Vec::new(),
        rank_bindings: vec![CephFsRankBinding {
            rank: RANK,
            gid: 123,
            incarnation: 4,
        }],
        daemons: vec![CephMdsDaemon {
            gid: 123,
            name: "mds-a".to_string(),
            rank: RANK as i32,
            incarnation: 4,
            state: CephMdsState::Active,
            state_sequence: 99,
        }],
        provenance: Vec::new(),
    }
}

fn layout() -> CephFsJournalLayout {
    CephFsJournalLayout {
        stripe_unit: 64 * 1024,
        stripe_count: 1,
        object_size: 64 * 1024,
        pool_id: POOL_ID,
    }
}

fn locator(object_name: String) -> CephFsObjectLocator {
    CephFsObjectLocator::new(1, POOL_ID, Vec::new(), object_name.into_bytes(), 17)
        .expect("valid locator")
}

fn pointer() -> Vec<u8> {
    let mut payload = FRONT_INODE.to_le_bytes().to_vec();
    payload.extend_from_slice(&0u64.to_le_bytes());
    envelope(1, 1, &payload)
}

fn header(layout: CephFsJournalLayout, expire_pos: u64, write_pos: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    append_string(&mut payload, CEPHFS_JOURNAL_MAGIC);
    payload.extend_from_slice(&layout.period().expect("period").to_le_bytes());
    payload.extend_from_slice(&expire_pos.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&write_pos.to_le_bytes());
    for value in [
        layout.stripe_unit,
        layout.stripe_count,
        layout.object_size,
        0,
        0,
        0,
        layout.pool_id as u32,
    ] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.push(1);
    envelope(2, 2, &payload)
}

fn lid_event(sequence: u64) -> Vec<u8> {
    versioned_event(101, &envelope(1, 1, &sequence.to_le_bytes()))
}

fn subtree_event_v6(sequence: u64) -> Vec<u8> {
    let mut payload = b"opaque-v6-fields".to_vec();
    payload.extend_from_slice(&sequence.to_le_bytes());
    versioned_event(2, &envelope(6, 5, &payload))
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
