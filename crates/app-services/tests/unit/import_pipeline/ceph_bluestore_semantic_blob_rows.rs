use ceph_wire::{
    BlueStoreAttributeSummary, BlueStoreBlob, BlueStoreBlobFlags, BlueStoreBlobIdentity,
    BlueStoreChecksum, BlueStoreChecksumType, BlueStoreExtentPayload, BlueStorePhysicalExtent,
};

use super::{write_blobs, FinalizedObjects, PayloadRef};

#[test]
fn persists_normalized_checksum_words_without_raw_byte_order() {
    let payload = PayloadRef {
        shard: None,
        payload: BlueStoreExtentPayload {
            version: 2,
            declared_extent_count: 0,
            encoded_length: 0,
            blobs: vec![checksum_blob()],
            extents: Vec::new(),
        },
    };
    let mut result = FinalizedObjects {
        objects: Vec::new(),
        onode_shards: Vec::new(),
        blobs: Vec::new(),
        checksum_chunks: Vec::new(),
        logical_extents: Vec::new(),
        physical_extents: Vec::new(),
    };

    write_blobs(
        "inventory-1",
        "object-1",
        0,
        &[],
        &[payload],
        0x20_000,
        &mut result,
    )
    .expect("persist normalized checksum");

    assert_eq!(result.blobs[0].checksum_value_count, 1);
    assert_eq!(result.checksum_chunks.len(), 1);
    assert_eq!(result.checksum_chunks[0].checksum_value, 0x1234_5678);
    assert_eq!(result.checksum_chunks[0].checksum_value_bytes, 4);
    assert_eq!(result.checksum_chunks[0].chunk_offset, 0);
    assert_eq!(result.checksum_chunks[0].chunk_length, 4096);
}

#[test]
fn checksum_rows_share_object_ordinals() {
    let mut blob = checksum_blob();
    blob.logical_length = 8192;
    blob.on_disk_length = 8192;
    blob.physical_extents[0].length = 8192;
    blob.checksum
        .as_mut()
        .expect("checksum metadata")
        .encoded_length = 8;
    blob.checksum_words.push(0x90ab_cdef);
    let payload = PayloadRef {
        shard: None,
        payload: BlueStoreExtentPayload {
            version: 2,
            declared_extent_count: 0,
            encoded_length: 0,
            blobs: vec![blob],
            extents: Vec::new(),
        },
    };
    let mut result = FinalizedObjects {
        objects: Vec::new(),
        onode_shards: Vec::new(),
        blobs: Vec::new(),
        checksum_chunks: Vec::new(),
        logical_extents: Vec::new(),
        physical_extents: Vec::new(),
    };

    write_blobs(
        "inventory-1",
        "object-1",
        7,
        &[],
        &[payload],
        0x20_000,
        &mut result,
    )
    .expect("persist shared checksum identifiers");

    assert_eq!(result.checksum_chunks.len(), 2);
    assert_eq!(result.checksum_chunks[0].object_ordinal, 7);
    assert_eq!(result.checksum_chunks[1].object_ordinal, 7);
}

#[test]
fn attribute_digest_changes_when_only_value_content_changes() {
    let first = BlueStoreAttributeSummary {
        name: b"key".to_vec(),
        value_length: 4,
        value_sha256: [1; 32],
    };
    let second = BlueStoreAttributeSummary {
        value_sha256: [2; 32],
        ..first.clone()
    };

    assert_ne!(
        super::super::digest::attributes_sha256(&[first]),
        super::super::digest::attributes_sha256(&[second])
    );
}

fn checksum_blob() -> BlueStoreBlob {
    BlueStoreBlob {
        identity: BlueStoreBlobIdentity::Local(0),
        owner: None,
        physical_extents: vec![BlueStorePhysicalExtent {
            offset: Some(0x10_000),
            length: 4096,
        }],
        on_disk_length: 4096,
        logical_length: 4096,
        compressed_length: None,
        flags: BlueStoreBlobFlags {
            raw: 4,
            legacy_mutable: false,
            compressed: false,
            checksum: true,
            has_unused: false,
            shared: false,
            unknown_bits: 0,
        },
        checksum: Some(BlueStoreChecksum {
            checksum_type: BlueStoreChecksumType::Crc32c,
            chunk_order: 12,
            encoded_length: 4,
            data_ceph_crc32c: 0,
            data_sha256: [0; 32],
        }),
        checksum_words: vec![0x1234_5678],
        unused_bitmap: None,
        shared_blob_id: None,
        use_tracker: None,
    }
}
