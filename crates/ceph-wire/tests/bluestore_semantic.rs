use ceph_wire::crc32c::ceph_crc32c;
use ceph_wire::{
    decode_bluestore_extent_payload, decode_bluestore_latest_value,
    decode_bluestore_latest_value_with_spanning_blobs, decode_bluestore_object_key,
    BlueStoreBlobIdentity, BlueStoreBlobUseTracker, BlueStoreChecksumType, BlueStoreCollectionId,
    BlueStoreCollectionKind, BlueStoreDecodedRecord, BlueStoreDeferredReason,
    BlueStoreExtentStorage, BlueStoreKeySpace, BlueStoreObjectRecord, BlueStoreOmapMode,
    BlueStoreOnodeTail, BlueStoreSemanticLimits, BlueStoreSuperRecord, CephWireError,
};

const BLOB_FLAG_COMPRESSED: u32 = 2;
const BLOB_FLAG_CHECKSUM: u32 = 4;
const BLOB_FLAG_UNUSED: u32 = 8;
const BLOB_FLAG_SHARED: u32 = 16;
const EXTENT_CONTIGUOUS: u64 = 1;
const EXTENT_ZERO_OFFSET: u64 = 2;
const EXTENT_SAME_LENGTH: u64 = 4;
const EXTENT_SPANNING: u64 = 8;

fn envelope(version: u8, payload: &[u8]) -> Vec<u8> {
    let mut encoded = vec![version, 1];
    encoded.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    encoded.extend_from_slice(payload);
    encoded
}

fn push_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn push_lowz(value: u64, output: &mut Vec<u8>) {
    let low_zero_nibbles = if value == 0 {
        0
    } else {
        (value.trailing_zeros() / 4).min(3)
    };
    push_varint(
        (value >> (low_zero_nibbles * 4)) << 2 | u64::from(low_zero_nibbles),
        output,
    );
}

fn push_denc_bytes(value: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    output.extend_from_slice(value);
}

fn push_lba(mut value: u64, output: &mut Vec<u8>) {
    let low_zero_nibbles = if value == 0 {
        0
    } else {
        value.trailing_zeros() / 4
    };
    let selector = low_zero_nibbles as i32 - 3;
    let (position, mut word) = if selector < 0 {
        (3, 7u32)
    } else if selector < 3 {
        value >>= low_zero_nibbles * 4;
        (selector as u32 + 1, (1u32 << selector) - 1)
    } else {
        value >>= 20;
        (3, 3u32)
    };
    word |= ((value << position) & 0x7fff_ffff) as u32;
    value >>= 31 - position;
    if value != 0 {
        word |= 0x8000_0000;
    }
    output.extend_from_slice(&word.to_le_bytes());
    while value != 0 {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
    }
}

fn push_blob(
    physical_extents: &[(u64, u32)],
    flags: u32,
    compressed_lengths: Option<(u32, u32)>,
    checksum: Option<(u8, u8, &[u8])>,
    unused: Option<u16>,
    shared_blob_id: Option<u64>,
    output: &mut Vec<u8>,
) {
    push_varint(physical_extents.len() as u64, output);
    for &(offset, length) in physical_extents {
        push_lba(offset, output);
        push_lowz(u64::from(length), output);
    }
    push_varint(u64::from(flags), output);
    if let Some((logical_length, compressed_length)) = compressed_lengths {
        push_lowz(u64::from(logical_length), output);
        push_lowz(u64::from(compressed_length), output);
    }
    if let Some((checksum_type, chunk_order, data)) = checksum {
        output.push(checksum_type);
        output.push(chunk_order);
        push_varint(data.len() as u64, output);
        output.extend_from_slice(data);
    }
    if let Some(unused) = unused {
        output.extend_from_slice(&unused.to_le_bytes());
    }
    if let Some(shared_blob_id) = shared_blob_id {
        output.extend_from_slice(&shared_blob_id.to_le_bytes());
    }
}

fn local_reuse_extent_payload(version: u8) -> Vec<u8> {
    let mut payload = vec![version];
    push_varint(2, &mut payload);
    push_varint(EXTENT_CONTIGUOUS | EXTENT_ZERO_OFFSET, &mut payload);
    push_lowz(0x800, &mut payload);
    push_blob(&[(0x4000, 0x1000)], 0, None, None, None, None, &mut payload);
    push_varint(
        (1 << 4) | EXTENT_CONTIGUOUS | EXTENT_SAME_LENGTH,
        &mut payload,
    );
    push_lowz(0x800, &mut payload);
    payload
}

fn spanning_blob_tail(version: u8, blob_id: u64) -> Vec<u8> {
    spanning_blob_tail_with_v2_refs(version, blob_id, &[0x400, 0x200])
}

fn spanning_blob_tail_with_v2_refs(version: u8, blob_id: u64, references: &[u32]) -> Vec<u8> {
    let mut tail = vec![version];
    push_varint(1, &mut tail);
    push_varint(blob_id, &mut tail);
    push_blob(&[(0x8000, 0x1000)], 0, None, None, None, None, &mut tail);
    if version == 2 {
        push_varint(0x800, &mut tail);
        push_varint(references.len() as u64, &mut tail);
        for &referenced_bytes in references {
            push_varint(u64::from(referenced_bytes), &mut tail);
        }
    } else {
        push_varint(2, &mut tail);
        push_lowz(0, &mut tail);
        push_lowz(0x400, &mut tail);
        push_varint(1, &mut tail);
        push_lowz(0x800, &mut tail);
        push_lowz(0x400, &mut tail);
        push_varint(2, &mut tail);
    }
    tail
}

fn spanning_extent_payload(version: u8, blob_id: u64) -> Vec<u8> {
    let mut payload = vec![version];
    push_varint(1, &mut payload);
    push_varint(
        (blob_id << 4) | EXTENT_SPANNING | EXTENT_CONTIGUOUS | EXTENT_ZERO_OFFSET,
        &mut payload,
    );
    push_lowz(0x800, &mut payload);
    payload
}

struct BlobSpec<'a> {
    physical_extents: &'a [(u64, u32)],
    flags: u32,
    compressed_lengths: Option<(u32, u32)>,
    checksum: Option<(u8, u8, &'a [u8])>,
    unused: Option<u16>,
    shared_blob_id: Option<u64>,
}

impl<'a> BlobSpec<'a> {
    fn new(physical_extents: &'a [(u64, u32)]) -> Self {
        Self {
            physical_extents,
            flags: 0,
            compressed_lengths: None,
            checksum: None,
            unused: None,
            shared_blob_id: None,
        }
    }
}

fn single_local_payload(
    version: u8,
    extent_blob_offset: u32,
    extent_length: u32,
    blob: BlobSpec<'_>,
) -> Vec<u8> {
    let mut payload = vec![version];
    push_varint(1, &mut payload);
    let mut extent_flags = EXTENT_CONTIGUOUS;
    if extent_blob_offset == 0 {
        extent_flags |= EXTENT_ZERO_OFFSET;
    }
    push_varint(extent_flags, &mut payload);
    if extent_blob_offset != 0 {
        push_lowz(u64::from(extent_blob_offset), &mut payload);
    }
    push_lowz(u64::from(extent_length), &mut payload);
    push_blob(
        blob.physical_extents,
        blob.flags,
        blob.compressed_lengths,
        blob.checksum,
        blob.unused,
        blob.shared_blob_id,
        &mut payload,
    );
    payload
}

fn push_escaped(value: &[u8], output: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in value {
        let marker = if byte <= b'#' || byte >= 0x80 {
            Some(b'#')
        } else if byte >= b'~' {
            Some(b'~')
        } else {
            None
        };
        if let Some(marker) = marker {
            output.push(marker);
            output.push(HEX[(byte >> 4) as usize]);
            output.push(HEX[(byte & 0x0f) as usize]);
        } else {
            output.push(byte);
        }
    }
    output.push(b'!');
}

fn object_key(
    namespace: &[u8],
    object_key: Option<&[u8]>,
    object_name: &[u8],
    snap: u64,
    generation: u64,
) -> Vec<u8> {
    let shard = -1i8;
    let pool = 7i64;
    let hash = 0x1234_5678u32;
    let mut key = Vec::new();
    key.push((shard as u8).wrapping_add(0x80));
    key.extend_from_slice(&(pool as u64).wrapping_add(1u64 << 63).to_be_bytes());
    key.extend_from_slice(&hash.reverse_bits().to_be_bytes());
    push_escaped(namespace, &mut key);
    if let Some(explicit_key) = object_key {
        push_escaped(explicit_key, &mut key);
        key.push(if explicit_key < object_name {
            b'<'
        } else {
            b'>'
        });
        push_escaped(object_name, &mut key);
    } else {
        push_escaped(object_name, &mut key);
        key.push(b'=');
    }
    key.extend_from_slice(&snap.to_be_bytes());
    key.extend_from_slice(&generation.to_be_bytes());
    key.push(b'o');
    key
}

fn cnode(bits: u32) -> Vec<u8> {
    envelope(1, &bits.to_le_bytes())
}

fn onode_value(
    version: u8,
    attributes: &[(&[u8], &[u8])],
    shards: &[(u32, u32)],
    zones: &[(u32, u64)],
    inline_extent_payload: Option<&[u8]>,
) -> Vec<u8> {
    onode_value_with_spanning(
        version,
        attributes,
        shards,
        zones,
        &[2, 0],
        inline_extent_payload,
    )
}

fn onode_value_with_spanning(
    version: u8,
    attributes: &[(&[u8], &[u8])],
    shards: &[(u32, u32)],
    zones: &[(u32, u64)],
    spanning_blobs: &[u8],
    inline_extent_payload: Option<&[u8]>,
) -> Vec<u8> {
    let mut payload = Vec::new();
    push_varint(17, &mut payload);
    push_varint(4096, &mut payload);
    payload.extend_from_slice(&(attributes.len() as u32).to_le_bytes());
    for (name, value) in attributes {
        push_denc_bytes(name, &mut payload);
        push_denc_bytes(value, &mut payload);
    }
    payload.push(0x0d);
    payload.extend_from_slice(&(shards.len() as u32).to_le_bytes());
    for &(offset, bytes) in shards {
        push_varint(u64::from(offset), &mut payload);
        push_varint(u64::from(bytes), &mut payload);
    }
    push_varint(8192, &mut payload);
    push_varint(4096, &mut payload);
    push_varint(5, &mut payload);
    if version >= 2 {
        payload.extend_from_slice(&(zones.len() as u32).to_le_bytes());
        for &(zone, offset) in zones {
            payload.extend_from_slice(&zone.to_le_bytes());
            payload.extend_from_slice(&offset.to_le_bytes());
        }
    }

    let mut value = envelope(version, &payload);
    value.extend_from_slice(spanning_blobs);
    if let Some(extents) = inline_extent_payload {
        push_denc_bytes(extents, &mut value);
    }
    value
}

fn decode(
    key_space: BlueStoreKeySpace,
    key: &[u8],
    value: &[u8],
) -> Result<BlueStoreDecodedRecord, CephWireError> {
    decode_bluestore_latest_value(key_space, key, value, BlueStoreSemanticLimits::default())
}

fn decoded_checksum_words(checksum_type: u8, data: &[u8]) -> Vec<u64> {
    let payload = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_CHECKSUM,
            checksum: Some((checksum_type, 12, data)),
            ..BlobSpec::new(&[(0x1000, 0x1000)])
        },
    );
    decode_bluestore_extent_payload(&payload, &[], BlueStoreSemanticLimits::default())
        .unwrap()
        .blobs
        .remove(0)
        .checksum_words
}

#[test]
fn decodes_known_super_values_and_defers_unknown_fields() {
    assert_eq!(
        decode(BlueStoreKeySpace::Super, b"nid_max", &42u64.to_le_bytes()).unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::NidMax(42))
    );
    assert_eq!(
        decode(
            BlueStoreKeySpace::Super,
            b"blobid_max",
            &99u64.to_le_bytes()
        )
        .unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::BlobIdMax(99))
    );
    assert_eq!(
        decode(
            BlueStoreKeySpace::Super,
            b"min_alloc_size",
            &4096u64.to_le_bytes()
        )
        .unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::MinAllocSize(4096))
    );
    assert_eq!(
        decode(
            BlueStoreKeySpace::Super,
            b"ondisk_format",
            &4i32.to_le_bytes()
        )
        .unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::OndiskFormat(4))
    );
    assert_eq!(
        decode(
            BlueStoreKeySpace::Super,
            b"min_compat_ondisk_format",
            &3i32.to_le_bytes()
        )
        .unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::MinCompatOndiskFormat(3))
    );
    assert_eq!(
        decode(BlueStoreKeySpace::Super, b"per_pool_omap", b"2").unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::PerPoolOmap(BlueStoreOmapMode::PerPg))
    );
    assert_eq!(
        decode(BlueStoreKeySpace::Super, b"freelist_type", b"bitmap").unwrap(),
        BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::FreelistType("bitmap".to_owned()))
    );

    let unknown = decode(BlueStoreKeySpace::Super, b"future_field", b"opaque").unwrap();
    let BlueStoreDecodedRecord::Super(BlueStoreSuperRecord::Unknown { field, deferred }) = unknown
    else {
        panic!("expected deferred super field");
    };
    assert_eq!(field, "future_field");
    assert_eq!(deferred.reason, BlueStoreDeferredReason::UnknownSuperField);
    assert_eq!(deferred.encoded_length, 6);
}

#[test]
fn decodes_prefix_stripped_canonical_collection_keys_and_cnode_bits() {
    let meta = decode(BlueStoreKeySpace::Collection, b"meta", &cnode(7)).unwrap();
    let BlueStoreDecodedRecord::Collection(meta) = meta else {
        panic!("expected collection");
    };
    assert_eq!(meta.collection, BlueStoreCollectionId::Meta);
    assert_eq!(meta.cnode.bits, 7);

    let head = decode(BlueStoreKeySpace::Collection, b"7.1a_head", &cnode(8)).unwrap();
    let BlueStoreDecodedRecord::Collection(head) = head else {
        panic!("expected collection");
    };
    assert_eq!(
        head.collection,
        BlueStoreCollectionId::Pg {
            pool: 7,
            seed: 0x1a,
            shard: None,
            kind: BlueStoreCollectionKind::Head,
        }
    );

    let temp = decode(BlueStoreKeySpace::Collection, b"7.1as3_TEMP", &cnode(9)).unwrap();
    let BlueStoreDecodedRecord::Collection(temp) = temp else {
        panic!("expected collection");
    };
    assert_eq!(
        temp.collection,
        BlueStoreCollectionId::Pg {
            pool: 7,
            seed: 0x1a,
            shard: Some(3),
            kind: BlueStoreCollectionKind::Temp,
        }
    );
}

#[test]
fn decodes_onode_and_extent_shard_keys_with_ceph_escapes() {
    let key = object_key(b"ns\0!\x80", Some(b"aaa"), b"obj~", 12, 34);
    let value = onode_value(2, &[], &[(0, 24)], &[], None);
    let decoded = decode(BlueStoreKeySpace::Object, &key, &value).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { object, .. } = *decoded else {
        panic!("expected onode");
    };
    assert_eq!(object.shard, -1);
    assert_eq!(object.pool, 7);
    assert_eq!(object.hash, 0x1234_5678);
    assert_eq!(object.namespace, b"ns\0!\x80");
    assert_eq!(object.object_key.as_deref(), Some(b"aaa".as_slice()));
    assert_eq!(object.object_name, b"obj~");
    assert_eq!(object.snap, 12);
    assert_eq!(object.generation, 34);

    let mut shard_key = key;
    shard_key.extend_from_slice(&0x1000u32.to_be_bytes());
    shard_key.push(b'x');
    let decoded = decode(BlueStoreKeySpace::Object, &shard_key, &[2, 0]).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::ExtentShard {
        shard_offset,
        payload,
        ..
    } = *decoded
    else {
        panic!("expected extent shard");
    };
    assert_eq!(shard_offset, 0x1000);
    assert_eq!(payload.declared_extent_count, 0);
    assert!(payload.extents.is_empty());
    assert!(payload.blobs.is_empty());
}

#[test]
fn rejects_raw_nul_truncation_and_wrong_object_suffix() {
    let key = object_key(b"", None, b"object", 0, 0);
    let value = onode_value(2, &[], &[], &[], Some(&[2, 0]));

    let mut raw_nul = key.clone();
    raw_nul[13] = 0;
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &raw_nul, &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &key[..10], &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));
    let mut wrong_suffix = key;
    *wrong_suffix.last_mut().unwrap() = b'z';
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &wrong_suffix, &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));
}

#[test]
fn rejects_noncanonical_object_shards_and_escape_forms() {
    let value = onode_value(2, &[], &[], &[], Some(&[2, 0]));

    let mut invalid_shard = object_key(b"", None, b"object", 0, 0);
    invalid_shard[0] = 0;
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &invalid_shard, &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));

    let mut wrong_high_byte_marker = object_key(b"\x80", None, b"object", 0, 0);
    wrong_high_byte_marker[13] = b'~';
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &wrong_high_byte_marker, &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));

    let mut uppercase_hex = object_key(b"\n", None, b"object", 0, 0);
    uppercase_hex[15] = b'A';
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &uppercase_hex, &value),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "object",
            ..
        })
    ));
}

#[test]
fn direct_object_key_decoder_enforces_logical_key_limit() {
    let key = object_key(b"", None, b"bounded", 0, 0);
    let limits = BlueStoreSemanticLimits {
        max_logical_key_bytes: key.len() - 1,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_object_key(&key, limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore logical key",
            length,
            limit,
        }) if length == key.len() && limit == key.len() - 1
    ));
}

#[test]
fn rejects_future_denc_versions_and_trailing_garbage() {
    let future = envelope(2, &7u32.to_le_bytes());
    assert!(matches!(
        decode(BlueStoreKeySpace::Collection, b"meta", &future),
        Err(CephWireError::UnsupportedBlueStoreDencVersion {
            context: "BlueStore cnode",
            encoded_version: 2,
            ..
        })
    ));

    let mut trailing = cnode(7);
    trailing.push(0xaa);
    assert!(matches!(
        decode(BlueStoreKeySpace::Collection, b"meta", &trailing),
        Err(CephWireError::BlueStoreTrailingBytes {
            context: "BlueStore cnode value",
            remaining: 1,
        })
    ));

    let key = object_key(b"", None, b"object", 0, 0);
    let inline_garbage = onode_value(2, &[], &[], &[], Some(&[2, 0, 0xff]));
    assert!(matches!(
        decode(BlueStoreKeySpace::Object, &key, &inline_garbage),
        Err(CephWireError::UnexpectedEof { .. })
    ));
}

#[test]
fn enforces_count_and_length_limits_before_consuming_payloads() {
    let key = object_key(b"", None, b"object", 0, 0);
    let mut count_payload = Vec::new();
    push_varint(1, &mut count_payload);
    push_varint(1, &mut count_payload);
    count_payload.extend_from_slice(&2u32.to_le_bytes());
    let count_value = envelope(2, &count_payload);
    let count_limits = BlueStoreSemanticLimits {
        max_attributes: 1,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_latest_value(BlueStoreKeySpace::Object, &key, &count_value, count_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore onode attributes",
            length: 2,
            limit: 1,
        })
    ));

    let mut length_payload = Vec::new();
    push_varint(1, &mut length_payload);
    push_varint(1, &mut length_payload);
    length_payload.extend_from_slice(&1u32.to_le_bytes());
    push_denc_bytes(b"a", &mut length_payload);
    length_payload.extend_from_slice(&4u32.to_le_bytes());
    let length_value = envelope(2, &length_payload);
    let length_limits = BlueStoreSemanticLimits {
        max_attribute_value_bytes: 3,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_latest_value(
            BlueStoreKeySpace::Object,
            &key,
            &length_value,
            length_limits
        ),
        Err(CephWireError::LengthLimit {
            context: "BlueStore attribute value",
            length: 4,
            limit: 3,
        })
    ));
}

#[test]
fn decodes_inline_and_sharded_onode_header_slices() {
    let key = object_key(b"", None, b"object", 0, 0);
    let inline_value = onode_value(
        2,
        &[(b"user.a", b"abc"), (b"user.b", b"12345")],
        &[],
        &[(3, 0x4000)],
        Some(&[2, 0]),
    );
    let inline = decode(BlueStoreKeySpace::Object, &key, &inline_value).unwrap();
    let BlueStoreDecodedRecord::Object(inline) = inline else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { onode, tail, .. } = *inline else {
        panic!("expected inline onode");
    };
    assert_eq!(onode.nid, 17);
    assert_eq!(onode.size, 4096);
    assert_eq!(onode.attributes[0].name, b"user.a");
    assert_eq!(onode.attributes[0].value_length, 3);
    assert_eq!(onode.flags.raw, 0x0d);
    assert_eq!(onode.allocation_hints.expected_object_size, 8192);
    assert_eq!(onode.zone_offset_refs[0].zone, 3);
    assert!(matches!(
        tail,
        BlueStoreOnodeTail::Decoded {
            extents: BlueStoreExtentStorage::Inline(_),
            ..
        }
    ));

    let sharded_value = onode_value(2, &[], &[(0, 32), (4096, 48)], &[], None);
    let sharded = decode(BlueStoreKeySpace::Object, &key, &sharded_value).unwrap();
    let BlueStoreDecodedRecord::Object(sharded) = sharded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { onode, tail, .. } = *sharded else {
        panic!("expected sharded onode");
    };
    assert_eq!(onode.extent_shards.len(), 2);
    assert_eq!(onode.extent_shards[1].offset, 4096);
    assert!(matches!(
        tail,
        BlueStoreOnodeTail::Decoded {
            extents: BlueStoreExtentStorage::Sharded,
            ..
        }
    ));
}

#[test]
fn attribute_summary_hashes_value_bytes_before_discarding_them() {
    let key = object_key(b"", None, b"attributes", 0, 0);
    let left_value = onode_value(2, &[(b"user.a", b"abc")], &[], &[], Some(&[2, 0]));
    let right_value = onode_value(2, &[(b"user.a", b"xyz")], &[], &[], Some(&[2, 0]));
    let decode_attribute = |value: &[u8]| {
        let decoded = decode(BlueStoreKeySpace::Object, &key, value).unwrap();
        let BlueStoreDecodedRecord::Object(record) = decoded else {
            panic!("expected object");
        };
        let BlueStoreObjectRecord::Onode { onode, .. } = *record else {
            panic!("expected onode");
        };
        onode.attributes.into_iter().next().unwrap()
    };
    let left = decode_attribute(&left_value);
    let right = decode_attribute(&right_value);
    let expected: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(b"abc").into();
    assert_eq!(left.name, right.name);
    assert_eq!(left.value_length, right.value_length);
    assert_eq!(left.value_sha256, expected);
    assert_ne!(left.value_sha256, right.value_sha256);
}

#[test]
fn decodes_extent_shard_payload_with_local_blob_reuse() {
    let key = object_key(b"", None, b"object", 0, 0);
    let mut shard_key = key;
    shard_key.extend_from_slice(&4096u32.to_be_bytes());
    shard_key.push(b'x');
    let encoded = local_reuse_extent_payload(2);
    let decoded = decode(BlueStoreKeySpace::Object, &shard_key, &encoded).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::ExtentShard { payload, .. } = *decoded else {
        panic!("expected extent shard");
    };
    assert_eq!(payload.declared_extent_count, 2);
    assert_eq!(payload.blobs.len(), 1);
    assert_eq!(payload.blobs[0].identity, BlueStoreBlobIdentity::Local(0));
    assert_eq!(payload.blobs[0].physical_extents[0].offset, Some(0x4000));
    assert_eq!(payload.extents[0].blob, BlueStoreBlobIdentity::Local(0));
    assert!(payload.extents[0].defines_blob);
    assert_eq!(payload.extents[1].blob, BlueStoreBlobIdentity::Local(0));
    assert!(!payload.extents[1].defines_blob);
    assert_eq!(payload.extents[1].logical_offset, 0x800);
    assert_eq!(payload.extents[1].blob_offset, 0x800);
}

#[test]
fn decodes_spanning_blob_and_contextual_extent_shard() {
    let key = object_key(b"", None, b"spanning", 0, 0);
    let tail = spanning_blob_tail(2, 7);
    let value = onode_value_with_spanning(2, &[], &[(0, 64)], &[], &tail, None);
    let decoded = decode(BlueStoreKeySpace::Object, &key, &value).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { object, tail, .. } = *decoded else {
        panic!("expected onode");
    };
    let BlueStoreOnodeTail::Decoded {
        spanning_blob_version,
        spanning_blobs,
        extents,
    } = tail;
    assert_eq!(spanning_blob_version, 2);
    assert_eq!(extents, BlueStoreExtentStorage::Sharded);
    assert_eq!(
        spanning_blobs[0].identity,
        BlueStoreBlobIdentity::Spanning(7)
    );
    assert_eq!(spanning_blobs[0].physical_extents[0].offset, Some(0x8000));
    assert_eq!(spanning_blobs[0].owner.as_deref(), Some(&object));
    assert!(matches!(
        &spanning_blobs[0].use_tracker,
        Some(BlueStoreBlobUseTracker::V2 {
            allocation_unit_size: 0x800,
            declared_allocation_units: 2,
            referenced_bytes,
        }) if referenced_bytes == &[0x400, 0x200]
    ));

    let payload = decode_bluestore_extent_payload(
        &spanning_extent_payload(2, 7),
        &spanning_blobs,
        BlueStoreSemanticLimits::default(),
    )
    .unwrap();
    assert_eq!(payload.extents[0].blob, BlueStoreBlobIdentity::Spanning(7));
    assert_eq!(payload.extents[0].logical_offset, 0);
    assert_eq!(payload.extents[0].length, 0x800);

    let mut shard_key = key;
    shard_key.extend_from_slice(&4096u32.to_be_bytes());
    shard_key.push(b'x');
    let decoded = decode_bluestore_latest_value_with_spanning_blobs(
        BlueStoreKeySpace::Object,
        &shard_key,
        &spanning_extent_payload(2, 7),
        &spanning_blobs,
        BlueStoreSemanticLimits::default(),
    )
    .unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::ExtentShard { payload, .. } = *decoded else {
        panic!("expected contextual extent shard");
    };
    assert_eq!(payload.extents[0].blob, BlueStoreBlobIdentity::Spanning(7));
}

#[test]
fn context_free_spanning_shard_is_deferred_and_wrong_owner_is_rejected() {
    let owner_key = object_key(b"", None, b"owner", 0, 0);
    let other_key = object_key(b"", None, b"other", 0, 0);
    let tail = spanning_blob_tail(2, 7);
    let other_value = onode_value_with_spanning(2, &[], &[(0, 64)], &[], &tail, None);
    let decoded = decode(BlueStoreKeySpace::Object, &other_key, &other_value).unwrap();
    let BlueStoreDecodedRecord::Object(record) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { tail, .. } = *record else {
        panic!("expected onode");
    };
    let BlueStoreOnodeTail::Decoded { spanning_blobs, .. } = tail;

    let mut shard_key = owner_key;
    shard_key.extend_from_slice(&4096u32.to_be_bytes());
    shard_key.push(b'x');
    let shard_value = spanning_extent_payload(2, 7);
    let deferred = decode(BlueStoreKeySpace::Object, &shard_key, &shard_value).unwrap();
    let BlueStoreDecodedRecord::Object(record) = deferred else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::DeferredExtentShard {
        shard_offset,
        payload,
        ..
    } = *record
    else {
        panic!("expected deferred extent shard");
    };
    assert_eq!(shard_offset, 4096);
    assert_eq!(
        payload.reason,
        BlueStoreDeferredReason::SpanningBlobContextRequired
    );
    assert_eq!(payload.encoded_length, shard_value.len());

    assert!(matches!(
        decode_bluestore_latest_value_with_spanning_blobs(
            BlueStoreKeySpace::Object,
            &shard_key,
            &shard_value,
            &spanning_blobs,
            BlueStoreSemanticLimits::default(),
        ),
        Err(CephWireError::BlueStoreSpanningBlobOwnerMismatch)
    ));

    let mut invalid_after_spanning = vec![2];
    push_varint(2, &mut invalid_after_spanning);
    push_varint(
        (7 << 4) | EXTENT_SPANNING | EXTENT_CONTIGUOUS | EXTENT_ZERO_OFFSET,
        &mut invalid_after_spanning,
    );
    push_lowz(0x100, &mut invalid_after_spanning);
    push_varint(
        (1 << 4) | EXTENT_CONTIGUOUS | EXTENT_ZERO_OFFSET | EXTENT_SAME_LENGTH,
        &mut invalid_after_spanning,
    );
    assert!(matches!(
        decode(
            BlueStoreKeySpace::Object,
            &shard_key,
            &invalid_after_spanning
        ),
        Err(CephWireError::MissingBlueStoreBlobReference {
            record_index: 1,
            kind: "local",
            blob_id: 0,
        })
    ));
}

#[test]
fn rejects_shared_blob_with_unused_bitmap() {
    let shared_blob_id = 0x1122_3344_5566_7788;
    let payload = single_local_payload(
        2,
        0,
        0x800,
        BlobSpec {
            flags: BLOB_FLAG_SHARED | BLOB_FLAG_UNUSED,
            unused: Some(0x005a),
            shared_blob_id: Some(shared_blob_id),
            ..BlobSpec::new(&[(0x9000, 0x1000)])
        },
    );
    assert!(matches!(
        decode_bluestore_extent_payload(&payload, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::InvalidBlueStoreSemanticValue {
            context: "BlueStore blob",
            reason: "shared blobs cannot carry an unused bitmap",
        })
    ));
}

#[test]
fn accepts_v2_use_tracker_reference_totals_above_allocation_unit_size() {
    let key = object_key(b"", None, b"tracker-references", 0, 0);
    let tail = spanning_blob_tail_with_v2_refs(2, 7, &[0x900, 0xa00]);
    let value = onode_value_with_spanning(2, &[], &[(0, 64)], &[], &tail, None);
    let decoded = decode(BlueStoreKeySpace::Object, &key, &value).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { tail, .. } = *decoded else {
        panic!("expected onode");
    };
    let BlueStoreOnodeTail::Decoded { spanning_blobs, .. } = tail;
    assert!(matches!(
        &spanning_blobs[0].use_tracker,
        Some(BlueStoreBlobUseTracker::V2 {
            allocation_unit_size: 0x800,
            declared_allocation_units: 2,
            referenced_bytes,
        }) if referenced_bytes == &[0x900, 0xa00]
    ));
}

#[test]
fn decodes_inline_compressed_checksum_blob_without_raw_checksum_bytes() {
    let checksum_data = [1, 2, 3, 4];
    let extent_payload = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_COMPRESSED | BLOB_FLAG_CHECKSUM,
            compressed_lengths: Some((0x2000, 0x900)),
            checksum: Some((4, 12, &checksum_data)),
            ..BlobSpec::new(&[(0xa000, 0x1000)])
        },
    );
    let key = object_key(b"", None, b"compressed", 0, 0);
    let value = onode_value(2, &[], &[], &[], Some(&extent_payload));
    let decoded = decode(BlueStoreKeySpace::Object, &key, &value).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::Onode { tail, .. } = *decoded else {
        panic!("expected onode");
    };
    let BlueStoreOnodeTail::Decoded {
        extents: BlueStoreExtentStorage::Inline(payload),
        ..
    } = tail
    else {
        panic!("expected inline extent payload");
    };
    let blob = &payload.blobs[0];
    assert_eq!(blob.logical_length, 0x2000);
    assert_eq!(blob.compressed_length, Some(0x900));
    assert_eq!(blob.on_disk_length, 0x1000);
    let checksum = blob.checksum.expect("checksum summary");
    assert_eq!(checksum.checksum_type, BlueStoreChecksumType::Crc32c);
    assert_eq!(checksum.chunk_order, 12);
    assert_eq!(checksum.encoded_length, checksum_data.len());
    assert_eq!(checksum.data_ceph_crc32c, ceph_crc32c(&checksum_data));
    assert_eq!(
        checksum.data_sha256,
        <sha2::Sha256 as sha2::Digest>::digest(checksum_data).as_slice()
    );
    assert_eq!(blob.checksum_words, [0x0403_0201]);
}

#[test]
fn normalizes_ceph_checksum_words_from_little_endian_storage() {
    assert_eq!(decoded_checksum_words(5, &[0x34, 0x12]), [0x1234]);
    assert_eq!(
        decoded_checksum_words(4, &[0x78, 0x56, 0x34, 0x12]),
        [0x1234_5678]
    );
    assert_eq!(
        decoded_checksum_words(3, &[0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]),
        [0x0123_4567_89ab_cdef]
    );
}

#[test]
fn rejects_checksum_count_mismatch_and_unallocated_logical_ranges() {
    let checksum_mismatch = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_CHECKSUM,
            checksum: Some((4, 12, &[1, 2, 3, 4])),
            ..BlobSpec::new(&[(0x1000, 0x2000)])
        },
    );
    assert!(matches!(
        decode_bluestore_extent_payload(
            &checksum_mismatch,
            &[],
            BlueStoreSemanticLimits::default()
        ),
        Err(CephWireError::InvalidBlueStoreChecksum {
            reason: "data length does not exactly match the on-disk chunk count",
            ..
        })
    ));

    let unallocated = single_local_payload(2, 0, 0x800, BlobSpec::new(&[(u64::MAX, 0x1000)]));
    assert!(matches!(
        decode_bluestore_extent_payload(&unallocated, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::InvalidBlueStoreExtent {
            record_index: 0,
            reason: "logical extent references an unallocated blob range",
        })
    ));
}

#[test]
fn rejects_invalid_physical_extent_and_unknown_checksum() {
    let invalid_extent = single_local_payload(2, 0, 1, BlobSpec::new(&[(0x1000, 0)]));
    assert!(matches!(
        decode_bluestore_extent_payload(&invalid_extent, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::InvalidBlueStorePhysicalExtent {
            index: 0,
            length: 0,
            ..
        })
    ));

    let unknown_checksum = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_CHECKSUM,
            checksum: Some((99, 12, &[])),
            ..BlobSpec::new(&[(0x1000, 0x1000)])
        },
    );
    assert!(matches!(
        decode_bluestore_extent_payload(&unknown_checksum, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::UnknownBlueStoreChecksumType { checksum_type: 99 })
    ));
}

#[test]
fn rejects_missing_blob_reference_and_blob_range_overflow() {
    let mut missing = vec![2];
    push_varint(1, &mut missing);
    push_varint(
        (1 << 4) | EXTENT_CONTIGUOUS | EXTENT_ZERO_OFFSET,
        &mut missing,
    );
    push_lowz(0x100, &mut missing);
    assert!(matches!(
        decode_bluestore_extent_payload(&missing, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::MissingBlueStoreBlobReference {
            record_index: 0,
            kind: "local",
            blob_id: 0,
        })
    ));

    let overflow = single_local_payload(2, 0x800, 0x1000, BlobSpec::new(&[(0x2000, 0x1000)]));
    assert!(matches!(
        decode_bluestore_extent_payload(&overflow, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::BlueStoreBlobRangeOverflow {
            record_index: 0,
            blob_offset: 0x800,
            length: 0x1000,
            logical_length: 0x1000,
        })
    ));
}

#[test]
fn validates_logical_non_overlap_after_decode() {
    let mut payload = decode_bluestore_extent_payload(
        &local_reuse_extent_payload(2),
        &[],
        BlueStoreSemanticLimits::default(),
    )
    .unwrap();
    payload.extents[1].logical_offset = 0x400;
    assert!(matches!(
        payload.validate_with_spanning_blobs(&[]),
        Err(CephWireError::BlueStoreLogicalExtentOverlap {
            previous_end: 0x800,
            logical_offset: 0x400,
        })
    ));
}

#[test]
fn rejects_extent_count_mismatch_and_truncation() {
    let mut count_mismatch = local_reuse_extent_payload(2);
    count_mismatch[1] = 1;
    assert!(matches!(
        decode_bluestore_extent_payload(&count_mismatch, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::BlueStoreExtentCountMismatch {
            declared: 1,
            decoded: 2,
        })
    ));

    let mut truncated = local_reuse_extent_payload(2);
    truncated.pop();
    assert!(matches!(
        decode_bluestore_extent_payload(&truncated, &[], BlueStoreSemanticLimits::default()),
        Err(CephWireError::UnexpectedEof { .. })
    ));

    let mut huge_truncated = vec![2];
    push_varint(u64::from(u32::MAX), &mut huge_truncated);
    let limits = BlueStoreSemanticLimits {
        max_extent_records: u32::MAX as usize,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&huge_truncated, &[], limits),
        Err(CephWireError::BlueStoreExtentCountMismatch {
            declared: u32::MAX,
            decoded: 0,
        })
    ));
}

#[test]
fn enforces_blob_resource_limits_before_allocation() {
    let physical = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec::new(&[(0x1000, 0x800), (0x2000, 0x800)]),
    );
    let physical_limits = BlueStoreSemanticLimits {
        max_physical_extents: 1,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&physical, &[], physical_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore physical extents",
            length: 2,
            limit: 1,
        })
    ));

    let blob_limits = BlueStoreSemanticLimits {
        max_blobs: 0,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&local_reuse_extent_payload(2), &[], blob_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore blobs",
            length: 1,
            limit: 0,
        })
    ));

    let checksum = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_CHECKSUM,
            checksum: Some((4, 12, &[1, 2, 3, 4])),
            ..BlobSpec::new(&[(0x1000, 0x1000)])
        },
    );
    let checksum_limits = BlueStoreSemanticLimits {
        max_checksum_bytes: 3,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&checksum, &[], checksum_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore checksum bytes",
            length: 4,
            limit: 3,
        })
    ));

    let key = object_key(b"", None, b"tracker-limit", 0, 0);
    let value = onode_value_with_spanning(2, &[], &[(0, 64)], &[], &spanning_blob_tail(2, 7), None);
    let tracker_limits = BlueStoreSemanticLimits {
        max_use_tracker_entries: 1,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_latest_value(BlueStoreKeySpace::Object, &key, &value, tracker_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore use tracker entries",
            length: 2,
            limit: 1,
        })
    ));
}

#[test]
fn enforces_aggregate_work_and_decoded_heap_budgets() {
    let payload = local_reuse_extent_payload(2);
    let work_limits = BlueStoreSemanticLimits {
        max_decode_work_units: payload.len() - 1,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&payload, &[], work_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore decode work units",
            length,
            limit,
        }) if length == payload.len() && limit == payload.len() - 1
    ));

    let checksum_data = vec![0x5a; 4096];
    let checksum_payload = single_local_payload(
        2,
        0,
        0x1000,
        BlobSpec {
            flags: BLOB_FLAG_CHECKSUM,
            checksum: Some((6, 0, &checksum_data)),
            ..BlobSpec::new(&[(0x1000, 0x1000)])
        },
    );
    let heap_limits = BlueStoreSemanticLimits {
        max_decoded_heap_bytes: 4096,
        ..BlueStoreSemanticLimits::default()
    };
    assert!(matches!(
        decode_bluestore_extent_payload(&checksum_payload, &[], heap_limits),
        Err(CephWireError::LengthLimit {
            context: "BlueStore decoded heap bytes",
            limit: 4096,
            ..
        })
    ));
}

#[test]
fn decodes_spanning_blob_v1_legacy_ref_map_and_v2_use_tracker() {
    let key = object_key(b"", None, b"trackers", 0, 0);
    for version in [1, 2] {
        let value = onode_value_with_spanning(
            2,
            &[],
            &[(0, 64)],
            &[],
            &spanning_blob_tail(version, 3),
            None,
        );
        let decoded = decode(BlueStoreKeySpace::Object, &key, &value).unwrap();
        let BlueStoreDecodedRecord::Object(decoded) = decoded else {
            panic!("expected object");
        };
        let BlueStoreObjectRecord::Onode { tail, .. } = *decoded else {
            panic!("expected onode");
        };
        let BlueStoreOnodeTail::Decoded { spanning_blobs, .. } = tail;
        match (version, spanning_blobs[0].use_tracker.as_ref()) {
            (1, Some(BlueStoreBlobUseTracker::V1LegacyRefMap { entries })) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].offset, 0);
                assert_eq!(entries[0].length, 0x400);
                assert_eq!(entries[0].refs, 1);
                assert_eq!(entries[1].offset, 0x800);
                assert_eq!(entries[1].length, 0x400);
                assert_eq!(entries[1].refs, 2);
            }
            (
                2,
                Some(BlueStoreBlobUseTracker::V2 {
                    allocation_unit_size,
                    declared_allocation_units,
                    referenced_bytes,
                }),
            ) => {
                assert_eq!(*allocation_unit_size, 0x800);
                assert_eq!(*declared_allocation_units, 2);
                assert_eq!(referenced_bytes, &[0x400, 0x200]);
            }
            _ => panic!("unexpected use tracker"),
        }
    }
}

#[test]
fn decodes_shared_blob_ref_map_and_rejects_delta_overflow() {
    let key = 0x1122_3344_5566_7788u64.to_be_bytes();
    let mut payload = Vec::new();
    push_varint(2, &mut payload);
    push_lowz(0x1000, &mut payload);
    push_lowz(0x100, &mut payload);
    push_varint(2, &mut payload);
    push_lowz(0x200, &mut payload);
    push_lowz(0x80, &mut payload);
    push_varint(1, &mut payload);
    let decoded = decode(BlueStoreKeySpace::SharedBlob, &key, &envelope(1, &payload)).unwrap();
    let BlueStoreDecodedRecord::SharedBlob(shared) = decoded else {
        panic!("expected shared blob");
    };
    assert_eq!(shared.sbid, u64::from_be_bytes(key));
    assert_eq!(shared.extent_refs.len(), 2);
    assert_eq!(shared.extent_refs[1].offset, 0x1200);

    let mut overflow = Vec::new();
    push_varint(2, &mut overflow);
    push_lowz((i64::MAX as u64) & !0xfff, &mut overflow);
    push_lowz(0x1000, &mut overflow);
    push_varint(1, &mut overflow);
    push_lowz(4096, &mut overflow);
    assert!(matches!(
        decode(BlueStoreKeySpace::SharedBlob, &key, &envelope(1, &overflow)),
        Err(CephWireError::IntegerOverflow {
            context: "BlueStore shared blob ref offset delta",
        })
    ));

    let zero_key = 0u64.to_be_bytes();
    assert!(matches!(
        decode(BlueStoreKeySpace::SharedBlob, &zero_key, &envelope(1, &[])),
        Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "shared blob",
            reason: "shared blob id must be non-zero",
        })
    ));

    let mut zero_ref = Vec::new();
    push_varint(1, &mut zero_ref);
    push_lowz(0x1000, &mut zero_ref);
    push_lowz(0x100, &mut zero_ref);
    push_varint(0, &mut zero_ref);
    assert!(matches!(
        decode(BlueStoreKeySpace::SharedBlob, &key, &envelope(1, &zero_ref)),
        Err(CephWireError::InvalidBlueStoreSemanticValue {
            context: "BlueStore shared blob ref map",
            reason: "extent refs must have non-zero length and reference count",
        })
    ));
}
