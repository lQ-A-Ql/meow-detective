use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, ReaderInfo};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreLogicalExtentRecord,
    CephBluestoreObjectReadPlan, CephBluestoreObjectRecord, CephBluestorePhysicalExtentRecord,
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
fn reads_allocated_extents_and_zero_fills_object_holes() {
    let mut device = vec![0u8; 64];
    device[16..20].copy_from_slice(b"ABCD");
    let plan = plan(
        12,
        vec![logical(4, 4, 0, 0)],
        vec![blob(0, 4, None, 0)],
        vec![physical(0, 0, Some(16), 4)],
        vec![],
    );
    let mut reader = RadosObjectReader::new(Box::new(MemoryReader::new(device)), plan).unwrap();

    let mut output = vec![0xff; 12];
    reader.read_exact(&mut output).unwrap();

    assert_eq!(&output[..4], &[0; 4]);
    assert_eq!(&output[4..8], b"ABCD");
    assert_eq!(&output[8..], &[0; 4]);
}

#[test]
fn supports_seek_and_partial_reads_across_physical_extents() {
    let mut device = vec![0u8; 64];
    device[8..10].copy_from_slice(b"AB");
    device[32..34].copy_from_slice(b"CD");
    let plan = plan(
        4,
        vec![logical(0, 4, 0, 0)],
        vec![blob(0, 4, None, 0)],
        vec![physical(0, 0, Some(8), 2), physical(0, 2, Some(32), 2)],
        vec![],
    );
    let mut reader = RadosObjectReader::new(Box::new(MemoryReader::new(device)), plan).unwrap();

    reader.seek(SeekFrom::Start(1)).unwrap();
    let mut output = [0u8; 2];
    reader.read_exact(&mut output).unwrap();

    assert_eq!(&output, b"BC");
}

#[test]
fn verifies_ceph_crc32c_before_returning_bytes() {
    let bytes = b"verified";
    let mut device = vec![0u8; 64];
    device[16..16 + bytes.len()].copy_from_slice(bytes);
    let checksum = ceph_wire::crc32c::ceph_crc32c(bytes);
    let plan = plan(
        bytes.len() as u64,
        vec![logical(0, bytes.len() as u64, 0, 0)],
        vec![blob(0, bytes.len() as u64, Some("crc32c"), 1)],
        vec![physical(0, 0, Some(16), bytes.len() as u64)],
        vec![checksum_row(0, 0, bytes.len() as u64, checksum)],
    );
    let mut reader = RadosObjectReader::new(Box::new(MemoryReader::new(device)), plan).unwrap();

    let mut output = vec![0u8; bytes.len()];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(output, bytes);
}

#[test]
fn rejects_checksum_mismatch_and_compressed_blobs() {
    let bytes = b"bad";
    let mut device = vec![0u8; 32];
    device[8..11].copy_from_slice(bytes);
    let mismatch = plan(
        3,
        vec![logical(0, 3, 0, 0)],
        vec![blob(0, 3, Some("crc32c"), 1)],
        vec![physical(0, 0, Some(8), 3)],
        vec![checksum_row(0, 0, 3, 0)],
    );
    let mut reader =
        RadosObjectReader::new(Box::new(MemoryReader::new(device.clone())), mismatch).unwrap();
    assert_eq!(
        reader.read_exact(&mut [0u8; 3]).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );

    let mut compressed_blob = blob(0, 3, None, 0);
    compressed_blob.flag_compressed = true;
    compressed_blob.flags_raw = 2;
    compressed_blob.compressed_length = Some(3);
    let compressed = plan(
        3,
        vec![logical(0, 3, 0, 0)],
        vec![compressed_blob],
        vec![physical(0, 0, Some(8), 3)],
        vec![],
    );
    assert!(matches!(
        RadosObjectReader::new(Box::new(MemoryReader::new(device)), compressed),
        Err(RadosReadError::Unsupported(_))
    ));
}

#[test]
fn rejects_cross_object_plan_identity_before_reading_device() {
    let mut plan = plan(
        1,
        vec![logical(0, 1, 0, 0)],
        vec![blob(0, 1, None, 0)],
        vec![physical(0, 0, Some(8), 1)],
        vec![],
    );
    plan.object.object_identity_sha256 = "b".repeat(64);

    assert!(matches!(
        RadosObjectReader::new(Box::new(MemoryReader::new(vec![0; 16])), plan),
        Err(RadosReadError::InvalidPlan(_))
    ));
}

fn plan(
    size: u64,
    logical_extents: Vec<CephBluestoreLogicalExtentRecord>,
    blobs: Vec<CephBluestoreBlobRecord>,
    physical_extents: Vec<CephBluestorePhysicalExtentRecord>,
    checksum_chunks: Vec<CephBluestoreChecksumChunkRecord>,
) -> CephBluestoreObjectReadPlan {
    let mut object = object(size);
    object.blob_count = blobs.len() as u64;
    object.logical_extent_count = logical_extents.len() as u64;
    object.physical_extent_count = physical_extents.len() as u64;
    CephBluestoreObjectReadPlan {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: "a".repeat(64),
        object_ordinal: 0,
        object,
        blobs,
        logical_extents,
        physical_extents,
        checksum_chunks,
    }
}

fn object(size: u64) -> CephBluestoreObjectRecord {
    CephBluestoreObjectRecord {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: "a".repeat(64),
        decoded_shard: -1,
        decoded_pool: 1,
        decoded_hash: 0,
        decoded_bitwise_hash: 0,
        object_namespace: vec![],
        object_key: None,
        object_name: b"object".to_vec(),
        snap_hex: "0000000000000000".to_string(),
        generation_hex: "0000000000000000".to_string(),
        onode_denc_version: 1,
        nid: 1,
        size,
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
    }
}

fn blob(
    ordinal: u32,
    length: u64,
    checksum_type: Option<&str>,
    checksum_count: u64,
) -> CephBluestoreBlobRecord {
    CephBluestoreBlobRecord {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: "a".repeat(64),
        blob_ordinal: ordinal,
        blob_kind: "local".to_string(),
        blob_id_hex: format!("{ordinal:016x}"),
        shared_blob_id_hex: None,
        logical_length: length,
        on_disk_length: length,
        compressed_length: None,
        flags_raw: u32::from(checksum_type.is_some()) * 4,
        flag_legacy_mutable: false,
        flag_compressed: false,
        flag_checksum: checksum_type.is_some(),
        flag_has_unused: false,
        flag_shared: false,
        flags_unknown_bits: 0,
        unused_bitmap: None,
        checksum_type: checksum_type.map(str::to_string),
        checksum_order: checksum_type.map(|_| 12),
        checksum_chunk_size: checksum_type.map(|_| length),
        checksum_encoded_length: checksum_type.map(|_| checksum_count * 4),
        checksum_value_count: checksum_count,
        checksum_data_crc32c: checksum_type.map(|_| 0),
        checksum_digest_sha256: checksum_type.map(|_| "c".repeat(64)),
        use_tracker_kind: None,
        use_tracker_allocation_unit_size: None,
        use_tracker_declared_allocation_units: None,
        use_tracker_entry_count: 0,
        use_tracker_sha256: None,
        logical_extent_count: 1,
        physical_extent_count: 1,
    }
}

fn logical(
    logical_offset: u64,
    length: u64,
    blob_ordinal: u32,
    blob_offset: u64,
) -> CephBluestoreLogicalExtentRecord {
    CephBluestoreLogicalExtentRecord {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: "a".repeat(64),
        extent_ordinal: logical_offset as u32,
        logical_offset,
        length,
        blob_ordinal,
        blob_offset,
        shard_ordinal: None,
        defines_blob: true,
        flags_raw: 0,
        flag_contiguous: false,
        flag_zero_blob_offset: false,
        flag_same_length: false,
        flag_spanning: false,
    }
}

fn physical(
    blob_ordinal: u32,
    blob_offset: u64,
    physical_offset: Option<u64>,
    length: u64,
) -> CephBluestorePhysicalExtentRecord {
    CephBluestorePhysicalExtentRecord {
        inventory_id: "inventory".to_string(),
        object_identity_sha256: "a".repeat(64),
        blob_ordinal,
        extent_ordinal: blob_offset as u32,
        blob_offset,
        device_id: 1,
        physical_offset_hex: physical_offset.map(|value| format!("{value:016x}")),
        length,
    }
}

fn checksum_row(
    blob_ordinal: u32,
    offset: u64,
    length: u64,
    value: u32,
) -> CephBluestoreChecksumChunkRecord {
    CephBluestoreChecksumChunkRecord {
        object_ordinal: 0,
        blob_ordinal,
        checksum_ordinal: 0,
        chunk_offset: offset,
        chunk_length: length,
        checksum_value: u64::from(value),
        checksum_value_bytes: 4,
    }
}
