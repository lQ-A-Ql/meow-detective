use ceph_wire::{
    decode_bluestore_omap_key, decode_bluestore_omap_logical_key, decode_bluestore_raw_omap_key,
    BlueStoreOmapKeyFamily, BlueStoreOmapKeyKind, BlueStoreOmapPool, CephWireError,
};

const NID: u64 = 0x0102_0304_0506_0708;
const PER_POOL_ID: i64 = -2;
const PER_PG_POOL_ID: u64 = 0x1112_1314_1516_1718;
const PER_PG_HASH: u32 = 0x2122_2324;

fn logical_key(family: BlueStoreOmapKeyFamily, marker: u8, user_key: &[u8]) -> Vec<u8> {
    let mut key = Vec::new();
    match family {
        BlueStoreOmapKeyFamily::Bulk | BlueStoreOmapKeyFamily::PgMeta => {}
        BlueStoreOmapKeyFamily::PerPool => {
            key.extend_from_slice(&PER_POOL_ID.to_be_bytes());
        }
        BlueStoreOmapKeyFamily::PerPg => {
            key.extend_from_slice(&PER_PG_POOL_ID.to_be_bytes());
            key.extend_from_slice(&PER_PG_HASH.to_be_bytes());
        }
    }
    key.extend_from_slice(&NID.to_be_bytes());
    key.push(marker);
    key.extend_from_slice(user_key);
    key
}

fn raw_key(family: BlueStoreOmapKeyFamily, logical_key: &[u8]) -> Vec<u8> {
    let mut raw = vec![family.prefix_byte(), 0];
    raw.extend_from_slice(logical_key);
    raw
}

fn assert_invalid(result: Result<ceph_wire::BlueStoreOmapKey<'_>, CephWireError>) {
    assert!(
        matches!(result, Err(CephWireError::InvalidBlueStoreOmapKey { .. })),
        "expected an OMAP-specific error, got {result:?}"
    );
}

#[test]
fn decodes_all_families_and_canonical_kinds() {
    let families = [
        BlueStoreOmapKeyFamily::Bulk,
        BlueStoreOmapKeyFamily::PgMeta,
        BlueStoreOmapKeyFamily::PerPool,
        BlueStoreOmapKeyFamily::PerPg,
    ];
    let kinds = [
        (b'-', &[][..]),
        (b'.', b"user\0key.-~\xff".as_slice()),
        (b'~', &[][..]),
    ];

    for family in families {
        for (marker, user_key) in kinds {
            let logical = logical_key(family, marker, user_key);
            let raw = raw_key(family, &logical);
            let decoded = decode_bluestore_raw_omap_key(&raw).expect("decode raw OMAP key");

            assert_eq!(decoded.family, family);
            assert_eq!(decoded.nid, NID);
            assert_eq!(decoded.hash, expected_hash(family));
            assert_eq!(decoded.pool, expected_pool(family));
            assert_eq!(decoded.kind.marker(), marker);
            assert_eq!(decoded.user_key(), (marker == b'.').then_some(user_key));

            let logical_decoded =
                decode_bluestore_omap_key(family, &logical).expect("decode logical OMAP key");
            let alias_decoded = decode_bluestore_omap_logical_key(family, &logical)
                .expect("decode logical OMAP key through alias");
            assert_eq!(logical_decoded, decoded);
            assert_eq!(alias_decoded, decoded);
        }
    }
}

fn expected_pool(family: BlueStoreOmapKeyFamily) -> Option<BlueStoreOmapPool> {
    match family {
        BlueStoreOmapKeyFamily::Bulk | BlueStoreOmapKeyFamily::PgMeta => None,
        BlueStoreOmapKeyFamily::PerPool => Some(BlueStoreOmapPool::PerPool(PER_POOL_ID)),
        BlueStoreOmapKeyFamily::PerPg => Some(BlueStoreOmapPool::PerPg(PER_PG_POOL_ID)),
    }
}

fn expected_hash(family: BlueStoreOmapKeyFamily) -> Option<u32> {
    (family == BlueStoreOmapKeyFamily::PerPg).then_some(PER_PG_HASH)
}

#[test]
fn preserves_empty_and_binary_entry_user_keys_without_copying() {
    for user_key in [&[][..], b"\0.-~\x80\xff".as_slice()] {
        let logical = logical_key(BlueStoreOmapKeyFamily::PerPg, b'.', user_key);
        let decoded = decode_bluestore_omap_key(BlueStoreOmapKeyFamily::PerPg, &logical)
            .expect("decode binary user key");
        let BlueStoreOmapKeyKind::Entry {
            user_key: decoded_user_key,
        } = decoded.kind
        else {
            panic!("expected entry");
        };

        assert_eq!(decoded_user_key, user_key);
        assert_eq!(
            decoded_user_key.as_ptr(),
            logical[logical.len() - user_key.len()..].as_ptr()
        );
    }
}

#[test]
fn decodes_big_endian_sortable_integer_boundaries() {
    for pool in [i64::MIN, -1, 0, i64::MAX] {
        let mut key = pool.to_be_bytes().to_vec();
        key.extend_from_slice(&u64::MAX.to_be_bytes());
        key.push(b'-');
        let decoded = decode_bluestore_omap_key(BlueStoreOmapKeyFamily::PerPool, &key)
            .expect("decode per-pool integer boundary");
        assert_eq!(decoded.pool, Some(BlueStoreOmapPool::PerPool(pool)));
        assert_eq!(decoded.nid, u64::MAX);
    }

    let mut key = u64::MAX.to_be_bytes().to_vec();
    key.extend_from_slice(&u32::MAX.to_be_bytes());
    key.extend_from_slice(&0u64.to_be_bytes());
    key.push(b'~');
    let decoded = decode_bluestore_omap_key(BlueStoreOmapKeyFamily::PerPg, &key)
        .expect("decode per-pg integer boundary");
    assert_eq!(decoded.pool, Some(BlueStoreOmapPool::PerPg(u64::MAX)));
    assert_eq!(decoded.hash, Some(u32::MAX));
    assert_eq!(decoded.nid, 0);
}

#[test]
fn rejects_raw_prefix_and_separator_errors() {
    assert_eq!(
        decode_bluestore_raw_omap_key(&[]),
        Err(CephWireError::InvalidBlueStoreOmapKey {
            family: "raw",
            reason: "key is truncated before the family and NUL separator",
        })
    );
    assert_invalid(decode_bluestore_raw_omap_key(b"M"));
    assert_invalid(decode_bluestore_raw_omap_key(b"Z\0payload"));
    assert_invalid(decode_bluestore_raw_omap_key(b"M!payload"));
}

#[test]
fn rejects_every_truncated_family_layout() {
    for family in [
        BlueStoreOmapKeyFamily::Bulk,
        BlueStoreOmapKeyFamily::PgMeta,
        BlueStoreOmapKeyFamily::PerPool,
        BlueStoreOmapKeyFamily::PerPg,
    ] {
        let complete = logical_key(family, b'-', &[]);
        for length in 0..complete.len() {
            assert_invalid(decode_bluestore_omap_key(family, &complete[..length]));
        }
    }
}

#[test]
fn rejects_unknown_markers_for_every_family() {
    for family in [
        BlueStoreOmapKeyFamily::Bulk,
        BlueStoreOmapKeyFamily::PgMeta,
        BlueStoreOmapKeyFamily::PerPool,
        BlueStoreOmapKeyFamily::PerPg,
    ] {
        for marker in [0, b'+', b'/', b'=', b'}', 0xff] {
            let key = logical_key(family, marker, &[]);
            assert_invalid(decode_bluestore_omap_key(family, &key));
        }
    }
}

#[test]
fn rejects_trailing_bytes_after_header_and_tail() {
    for family in [
        BlueStoreOmapKeyFamily::Bulk,
        BlueStoreOmapKeyFamily::PgMeta,
        BlueStoreOmapKeyFamily::PerPool,
        BlueStoreOmapKeyFamily::PerPg,
    ] {
        for marker in [b'-', b'~'] {
            let key = logical_key(family, marker, b"trailing");
            assert_invalid(decode_bluestore_omap_key(family, &key));
        }
    }
}

#[test]
fn rejects_non_canonical_cross_family_layouts() {
    let bulk = logical_key(BlueStoreOmapKeyFamily::Bulk, b'-', &[]);
    let per_pool = logical_key(BlueStoreOmapKeyFamily::PerPool, b'-', &[]);
    let per_pg = logical_key(BlueStoreOmapKeyFamily::PerPg, b'-', &[]);

    assert_invalid(decode_bluestore_omap_key(
        BlueStoreOmapKeyFamily::PerPool,
        &bulk,
    ));
    assert_invalid(decode_bluestore_omap_key(
        BlueStoreOmapKeyFamily::PerPg,
        &bulk,
    ));
    assert_invalid(decode_bluestore_omap_key(
        BlueStoreOmapKeyFamily::Bulk,
        &per_pool,
    ));
    assert_invalid(decode_bluestore_omap_key(
        BlueStoreOmapKeyFamily::PerPool,
        &per_pg,
    ));
}
