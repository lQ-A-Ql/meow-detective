use ceph_wire::{
    decode_bluestore_latest_value, BlueStoreCollectionId, BlueStoreCollectionKind,
    BlueStoreDecodedRecord, BlueStoreDeferredReason, BlueStoreExtentStorage, BlueStoreKeySpace,
    BlueStoreObjectRecord, BlueStoreOmapMode, BlueStoreOnodeTail, BlueStorePayloadStatus,
    BlueStoreSemanticLimits, BlueStoreSuperRecord, CephWireError,
};

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
    value.push(2);
    push_varint(0, &mut value);
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
    assert_eq!(payload.status, BlueStorePayloadStatus::Parsed);
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
        Err(CephWireError::BlueStoreTrailingBytes {
            context: "BlueStore empty extent map",
            remaining: 1,
        })
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
fn marks_nonempty_extent_payloads_as_typed_deferred() {
    let key = object_key(b"", None, b"object", 0, 0);
    let mut shard_key = key;
    shard_key.extend_from_slice(&4096u32.to_be_bytes());
    shard_key.push(b'x');
    let decoded = decode(BlueStoreKeySpace::Object, &shard_key, &[2, 1]).unwrap();
    let BlueStoreDecodedRecord::Object(decoded) = decoded else {
        panic!("expected object");
    };
    let BlueStoreObjectRecord::ExtentShard { payload, .. } = *decoded else {
        panic!("expected extent shard");
    };
    let BlueStorePayloadStatus::Deferred(deferred) = payload.status else {
        panic!("expected deferred extent records");
    };
    assert_eq!(deferred.reason, BlueStoreDeferredReason::ExtentRecords);
    assert_eq!(deferred.encoded_length, 2);
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
    push_lowz(0, &mut overflow);
    push_varint(1, &mut overflow);
    push_lowz(4096, &mut overflow);
    assert!(matches!(
        decode(BlueStoreKeySpace::SharedBlob, &key, &envelope(1, &overflow)),
        Err(CephWireError::IntegerOverflow {
            context: "BlueStore shared blob ref offset delta",
        })
    ));
}
