use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, ReaderInfo};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreLogicalExtentRecord, CephBluestoreObjectReadPlan,
    CephBluestoreObjectRecord, CephBluestorePhysicalExtentRecord,
};

use super::*;

struct MemoryReader {
    cursor: Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            cursor: Cursor::new(bytes),
            info: ReaderInfo {
                path: PathBuf::from("memory"),
                size,
                kind: "memory".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(output)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn short_rados_object_tail_is_zero_filled_to_the_requested_rbd_range() {
    let mut device = vec![0u8; 32];
    device[8..12].copy_from_slice(b"DATA");
    let plan =
        RadosObjectReader::prepare_layout(&short_object_plan()).expect("prepare object plan");
    let reader = SharedEvidenceReader::new(Box::new(MemoryReader::new(device)));

    let bytes = read_plan_page(reader, plan, 2, 8).expect("read zero-padded object tail");

    assert_eq!(&bytes[..2], b"TA");
    assert_eq!(&bytes[2..], &[0; 6]);
}

fn short_object_plan() -> CephBluestoreObjectReadPlan {
    let identity = "a".repeat(64);
    CephBluestoreObjectReadPlan {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: identity.clone(),
        object_ordinal: 0,
        object: CephBluestoreObjectRecord {
            inventory_id: "inventory".to_string(),
            object_identity_sha256: identity.clone(),
            decoded_shard: -1,
            decoded_pool: 1,
            decoded_hash: 0,
            decoded_bitwise_hash: 0,
            object_namespace: Vec::new(),
            object_key: None,
            object_name: b"object".to_vec(),
            snap_hex: "0000000000000000".to_string(),
            generation_hex: "0000000000000000".to_string(),
            onode_denc_version: 1,
            nid: 1,
            size: 4,
            flags_raw: 0,
            flag_omap: false,
            flag_pgmeta_omap: false,
            flag_per_pool_omap: false,
            flag_per_pg_omap: false,
            flags_unknown_bits: 0,
            attribute_count: 0,
            attribute_value_bytes: 0,
            attributes_sha256: "b".repeat(64),
            expected_object_size: 0,
            expected_write_size: 0,
            allocation_hint_flags: 0,
            zone_ref_count: 0,
            extent_storage: "inline".to_string(),
            spanning_blob_version: 1,
            declared_spanning_blob_count: 0,
            decode_status: "parsed".to_string(),
            deferred_reason: None,
            onode_shard_count: 0,
            blob_count: 1,
            logical_extent_count: 1,
            physical_extent_count: 1,
        },
        blobs: vec![CephBluestoreBlobRecord {
            inventory_id: "inventory".to_string(),
            object_identity_sha256: identity.clone(),
            blob_ordinal: 0,
            blob_kind: "local".to_string(),
            blob_id_hex: "0000000000000000".to_string(),
            shared_blob_id_hex: None,
            logical_length: 4,
            on_disk_length: 4,
            compressed_length: None,
            flags_raw: 0,
            flag_legacy_mutable: false,
            flag_compressed: false,
            flag_checksum: false,
            flag_has_unused: false,
            flag_shared: false,
            flags_unknown_bits: 0,
            unused_bitmap: None,
            checksum_type: None,
            checksum_order: None,
            checksum_chunk_size: None,
            checksum_encoded_length: None,
            checksum_value_count: 0,
            checksum_data_crc32c: None,
            checksum_digest_sha256: None,
            use_tracker_kind: None,
            use_tracker_allocation_unit_size: None,
            use_tracker_declared_allocation_units: None,
            use_tracker_entry_count: 0,
            use_tracker_sha256: None,
            logical_extent_count: 1,
            physical_extent_count: 1,
        }],
        logical_extents: vec![CephBluestoreLogicalExtentRecord {
            inventory_id: "inventory".to_string(),
            object_identity_sha256: identity.clone(),
            extent_ordinal: 0,
            logical_offset: 0,
            length: 4,
            blob_ordinal: 0,
            blob_offset: 0,
            shard_ordinal: None,
            defines_blob: true,
            flags_raw: 0,
            flag_contiguous: false,
            flag_zero_blob_offset: false,
            flag_same_length: false,
            flag_spanning: false,
        }],
        physical_extents: vec![CephBluestorePhysicalExtentRecord {
            inventory_id: "inventory".to_string(),
            object_identity_sha256: identity,
            blob_ordinal: 0,
            extent_ordinal: 0,
            blob_offset: 0,
            device_id: 1,
            physical_offset_hex: Some("0000000000000008".to_string()),
            length: 4,
        }],
        checksum_chunks: Vec::new(),
    }
}
