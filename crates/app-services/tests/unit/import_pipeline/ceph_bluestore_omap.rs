use ceph_wire::{BlueStoreOmapKeyFamily, CephEncode};

use super::{
    BlueStoreOmapError, BlueStoreOmapFragment, BlueStoreOmapLimits, BlueStoreOmapOwnerKind,
};
use crate::import_pipeline::ceph_bluestore_omap::types::BlueStoreOmapPoolScope;
use ceph_wire::{BlueStoreObjectId, BlueStoreOnodeFlags, BlueStoreOnodeHeader};

const NID: u64 = 0x0102_0304_0506_0708;

fn logical_key(family: BlueStoreOmapKeyFamily, nid: u64, marker: u8, user_key: &[u8]) -> Vec<u8> {
    logical_key_with_scope(family, 0, 0, nid, marker, user_key)
}

fn logical_key_with_scope(
    family: BlueStoreOmapKeyFamily,
    pool: i64,
    hash: u32,
    nid: u64,
    marker: u8,
    user_key: &[u8],
) -> Vec<u8> {
    let mut key = Vec::new();
    match family {
        BlueStoreOmapKeyFamily::Bulk | BlueStoreOmapKeyFamily::PgMeta => {}
        BlueStoreOmapKeyFamily::PerPool => key.extend_from_slice(&pool.to_be_bytes()),
        BlueStoreOmapKeyFamily::PerPg => {
            key.extend_from_slice(&(pool as u64).to_be_bytes());
            key.extend_from_slice(&hash.to_be_bytes());
        }
    }
    key.extend_from_slice(&nid.to_be_bytes());
    key.push(marker);
    key.extend_from_slice(user_key);
    key
}

fn encoded_string(value: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    value.to_string().encode(&mut encoded);
    encoded
}

fn encoded<T: CephEncode>(value: T) -> Vec<u8> {
    let mut encoded = Vec::new();
    value.encode(&mut encoded);
    encoded
}

fn object(name: &[u8]) -> BlueStoreObjectId {
    BlueStoreObjectId {
        shard: 0,
        pool: 1,
        hash: 2,
        bitwise_hash: 3,
        namespace: Vec::new(),
        object_key: None,
        object_name: name.to_vec(),
        snap: 0,
        generation: 0,
    }
}

fn onode(nid: u64, flags: BlueStoreOnodeFlags) -> BlueStoreOnodeHeader {
    BlueStoreOnodeHeader {
        denc_version: 1,
        nid,
        size: 0,
        attributes: Vec::new(),
        flags,
        extent_shards: Vec::new(),
        allocation_hints: ceph_wire::BlueStoreAllocationHints {
            expected_object_size: 0,
            expected_write_size: 0,
            flags: 0,
        },
        zone_offset_refs: Vec::new(),
    }
}

fn flags(
    omap: bool,
    pgmeta_omap: bool,
    per_pool_omap: bool,
    per_pg_omap: bool,
) -> BlueStoreOnodeFlags {
    BlueStoreOnodeFlags {
        raw: 0,
        omap,
        pgmeta_omap,
        per_pool_omap,
        per_pg_omap,
        unknown_bits: 0,
    }
}

fn observe(
    fragment: &mut BlueStoreOmapFragment,
    family: BlueStoreOmapKeyFamily,
    nid: u64,
    marker: u8,
    user_key: &[u8],
    value: &[u8],
) -> Result<(), BlueStoreOmapError> {
    let key = logical_key(family, nid, marker, user_key);
    fragment.observe_routed_latest_value(family, &key, value)
}

#[test]
fn closes_all_omap_families_and_preserves_scope_identity() {
    let mut fragment = BlueStoreOmapFragment::default();
    for (family, pool, hash) in [
        (BlueStoreOmapKeyFamily::Bulk, 0, 0),
        (BlueStoreOmapKeyFamily::PgMeta, 0, 0),
        (BlueStoreOmapKeyFamily::PerPool, -2, 0),
        (BlueStoreOmapKeyFamily::PerPg, 7, 0x1122_3344),
    ] {
        let header = logical_key_with_scope(family, pool, hash, NID, b'-', &[]);
        let tail = logical_key_with_scope(family, pool, hash, NID, b'~', &[]);
        fragment
            .observe_routed_latest_value(family, &header, &[])
            .expect("header");
        fragment
            .observe_routed_latest_value(family, &tail, &[])
            .expect("tail");
    }

    let snapshot = fragment.finish().expect("closed OMAP snapshot");
    assert_eq!(snapshot.scopes.len(), 4);
    assert_eq!(
        snapshot.scopes[0].scope.family,
        BlueStoreOmapKeyFamily::Bulk
    );
    assert_eq!(
        snapshot.scopes[2].scope.pool,
        Some(BlueStoreOmapPoolScope::PerPool(-2))
    );
    assert_eq!(
        snapshot.scopes[3].scope.pool,
        Some(BlueStoreOmapPoolScope::PerPg(7))
    );
    assert_eq!(snapshot.scopes[3].scope.hash, Some(0x1122_3344));
}

#[test]
fn extracts_rbd_directory_and_header_fields_with_owner_binding() {
    let mut fragment = BlueStoreOmapFragment::default();
    let directory_nid = 11;
    let header_nid = 12;
    fragment
        .observe_onode(
            &object(b"rbd_directory"),
            &onode(directory_nid, flags(true, false, false, false)),
        )
        .expect("directory owner");
    fragment
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(header_nid, flags(true, false, false, false)),
        )
        .expect("header owner");

    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        directory_nid,
        b'-',
        &[],
        &[],
    )
    .expect("directory header");
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        directory_nid,
        b'.',
        b"name_vm-disk",
        &encoded_string("image-id"),
    )
    .expect("name mapping");
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        directory_nid,
        b'.',
        b"id_image-id",
        &encoded_string("vm-disk"),
    )
    .expect("id mapping");
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        directory_nid,
        b'~',
        &[],
        &[],
    )
    .expect("directory tail");

    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        header_nid,
        b'-',
        &[],
        &[],
    )
    .expect("header header");
    for (key, value) in [
        (b"size".as_slice(), encoded(0x1000_u64)),
        (b"order".as_slice(), encoded(22_u8)),
        (b"features".as_slice(), encoded(0x121_u64)),
        (b"op_features".as_slice(), encoded(0x4_u64)),
        (b"parent".as_slice(), b"opaque-parent".to_vec()),
        (
            b"object_prefix".as_slice(),
            encoded_string("rbd_data.image-id"),
        ),
        (b"stripe_unit".as_slice(), encoded(4096_u64)),
        (b"stripe_count".as_slice(), encoded(1_u64)),
        (b"data_pool_id".as_slice(), encoded(8_i64)),
    ] {
        observe(
            &mut fragment,
            BlueStoreOmapKeyFamily::Bulk,
            header_nid,
            b'.',
            key,
            &value,
        )
        .expect("header field");
    }
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        header_nid,
        b'~',
        &[],
        &[],
    )
    .expect("header tail");

    let snapshot = fragment.finish().expect("RBD snapshot");
    assert_eq!(snapshot.directory_mappings.len(), 1);
    assert_eq!(snapshot.directory_mappings[0].image_name, "vm-disk");
    assert_eq!(snapshot.directory_mappings[0].image_id, "image-id");
    assert!(snapshot.directory_mappings[0].bidirectional);

    assert_eq!(snapshot.rbd_headers.len(), 1);
    let header = &snapshot.rbd_headers[0];
    assert_eq!(header.image_id, "image-id");
    assert_eq!(header.owner_nid, header_nid);
    assert_eq!(header.size, Some(0x1000));
    assert_eq!(header.order, Some(22));
    assert_eq!(header.features, Some(0x121));
    assert_eq!(header.operation_features, Some(0x4));
    assert!(header.parent_key_present);
    assert_eq!(header.object_prefix.as_deref(), Some("rbd_data.image-id"));
    assert_eq!(header.stripe_unit, Some(4096));
    assert_eq!(header.stripe_count, Some(1));
    assert_eq!(header.data_pool_id, Some(8));
}

#[test]
fn binds_omap_scope_only_to_the_matching_flag_family() {
    let mut fragment = BlueStoreOmapFragment::default();
    let nid = 21;
    fragment
        .observe_onode(
            &object(b"rbd_header.family-test"),
            &onode(nid, flags(true, false, true, false)),
        )
        .expect("owner");
    let key = logical_key_with_scope(BlueStoreOmapKeyFamily::PerPool, 3, 0, nid, b'-', &[]);
    fragment
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::PerPool, &key, &[])
        .expect("header");
    let key = logical_key_with_scope(BlueStoreOmapKeyFamily::PerPool, 3, 0, nid, b'~', &[]);
    fragment
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::PerPool, &key, &[])
        .expect("tail");

    let snapshot = fragment.finish().expect("snapshot");
    assert_eq!(
        snapshot.scopes[0].owner.as_ref().unwrap().family,
        BlueStoreOmapKeyFamily::PerPool
    );
    assert!(matches!(
        snapshot.scopes[0].owner.as_ref().unwrap().kind,
        BlueStoreOmapOwnerKind::RbdHeader { .. }
    ));
    assert_eq!(snapshot.rbd_headers[0].image_id, "family-test");
}

#[test]
fn rejects_unclosed_and_out_of_order_scopes() {
    let mut fragment = BlueStoreOmapFragment::default();
    let entry = logical_key(BlueStoreOmapKeyFamily::Bulk, NID, b'.', b"size");
    fragment
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::Bulk, &entry, &encoded(1_u64))
        .expect("headerless entry");
    assert!(matches!(
        fragment.finish(),
        Err(BlueStoreOmapError::UnclosedScope { .. })
    ));
}

#[test]
fn accepts_headerless_pg_metadata_scopes_without_rbd_projection() {
    for (family, pool, hash) in [
        (BlueStoreOmapKeyFamily::PgMeta, 0, 0),
        (BlueStoreOmapKeyFamily::PerPg, 7, 0x1122_3344),
    ] {
        let mut fragment = BlueStoreOmapFragment::default();
        let entry = logical_key_with_scope(family, pool, hash, NID, b'.', b"size");
        fragment
            .observe_routed_latest_value(family, &entry, b"not-rbd-encoded")
            .expect("headerless PG entry");
        let tail = logical_key_with_scope(family, pool, hash, NID, b'~', &[]);
        fragment
            .observe_routed_latest_value(family, &tail, &[])
            .expect("headerless PG tail");

        let snapshot = fragment.finish().expect("headerless PG scope");
        assert_eq!(snapshot.scopes.len(), 1);
        assert_eq!(snapshot.scopes[0].entry_count, 1);
        assert_eq!(snapshot.scopes[0].recognized_entry_count, 0);
        assert!(snapshot.scopes[0].owner.is_none());
        assert!(snapshot.directory_mappings.is_empty());
        assert!(snapshot.rbd_headers.is_empty());
    }
}

#[test]
fn accepts_headerless_bulk_and_per_pool_scopes_without_rbd_projection() {
    for (family, pool) in [
        (BlueStoreOmapKeyFamily::Bulk, 0),
        (BlueStoreOmapKeyFamily::PerPool, 7),
    ] {
        let mut fragment = BlueStoreOmapFragment::default();
        let entry = logical_key_with_scope(family, pool, 0, NID, b'.', b"size");
        fragment
            .observe_routed_latest_value(family, &entry, b"not-rbd-encoded")
            .expect("headerless entry");
        let tail = logical_key_with_scope(family, pool, 0, NID, b'~', &[]);
        fragment
            .observe_routed_latest_value(family, &tail, &[])
            .expect("headerless tail");
        let snapshot = fragment.finish().expect("headerless scope");
        assert_eq!(snapshot.scopes[0].entry_count, 1);
        assert_eq!(snapshot.scopes[0].recognized_entry_count, 0);
        assert!(snapshot.scopes[0].owner.is_none());
    }
}

#[test]
fn binds_headerless_per_pg_rbd_entries_after_cross_fragment_merge() {
    let directory_nid = 31;
    let header_nid = 32;
    let mut owners = BlueStoreOmapFragment::default();
    owners
        .observe_onode(
            &object(b"rbd_directory"),
            &onode(directory_nid, flags(true, false, true, true)),
        )
        .expect("directory owner");
    owners
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(header_nid, flags(true, false, true, true)),
        )
        .expect("header owner");

    let mut entries = BlueStoreOmapFragment::default();
    for (nid, key, value) in [
        (
            directory_nid,
            b"name_vm-disk".as_slice(),
            encoded_string("image-id"),
        ),
        (
            directory_nid,
            b"id_image-id".as_slice(),
            encoded_string("vm-disk"),
        ),
        (header_nid, b"size".as_slice(), encoded(0x20_0000_u64)),
        (header_nid, b"order".as_slice(), encoded(22_u8)),
        (
            header_nid,
            b"object_prefix".as_slice(),
            encoded_string("rbd_data.image-id"),
        ),
    ] {
        let key = logical_key_with_scope(
            BlueStoreOmapKeyFamily::PerPg,
            2,
            0x1122_3344,
            nid,
            b'.',
            key,
        );
        entries
            .observe_routed_latest_value(BlueStoreOmapKeyFamily::PerPg, &key, &value)
            .expect("headerless RBD entry");
    }
    for nid in [directory_nid, header_nid] {
        let tail = logical_key_with_scope(
            BlueStoreOmapKeyFamily::PerPg,
            2,
            0x1122_3344,
            nid,
            b'~',
            &[],
        );
        entries
            .observe_routed_latest_value(BlueStoreOmapKeyFamily::PerPg, &tail, &[])
            .expect("headerless RBD tail");
    }

    owners.merge(entries).expect("merge column fragments");
    let snapshot = owners.finish().expect("RBD snapshot");
    assert_eq!(snapshot.directory_mappings.len(), 1);
    assert_eq!(snapshot.directory_mappings[0].image_name, "vm-disk");
    assert_eq!(snapshot.directory_mappings[0].image_id, "image-id");
    assert!(snapshot.directory_mappings[0].bidirectional);
    assert_eq!(snapshot.rbd_headers.len(), 1);
    assert_eq!(snapshot.rbd_headers[0].image_id, "image-id");
    assert_eq!(snapshot.rbd_headers[0].size, Some(0x20_0000));
    assert_eq!(snapshot.rbd_headers[0].order, Some(22));
    assert_eq!(
        snapshot.rbd_headers[0].object_prefix.as_deref(),
        Some("rbd_data.image-id")
    );
    assert!(snapshot
        .scopes
        .iter()
        .all(|scope| scope.owner.is_some() && scope.recognized_entry_count > 0));
}

#[test]
fn selects_the_effective_omap_family_using_ceph_precedence() {
    let mut fragment = BlueStoreOmapFragment::default();
    fragment
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(NID, flags(true, false, true, true)),
        )
        .expect("owner");

    for family in [
        BlueStoreOmapKeyFamily::PerPool,
        BlueStoreOmapKeyFamily::PerPg,
    ] {
        let key = logical_key_with_scope(family, 2, 0x1122_3344, NID, b'~', &[]);
        fragment
            .observe_routed_latest_value(family, &key, &[])
            .expect("tail");
    }

    let snapshot = fragment.finish().expect("snapshot");
    let per_pool = snapshot
        .scopes
        .iter()
        .find(|scope| scope.scope.family == BlueStoreOmapKeyFamily::PerPool)
        .expect("per-pool scope");
    let per_pg = snapshot
        .scopes
        .iter()
        .find(|scope| scope.scope.family == BlueStoreOmapKeyFamily::PerPg)
        .expect("per-pg scope");
    assert!(per_pool.owner.is_none());
    assert_eq!(
        per_pg.owner.as_ref().map(|owner| owner.family),
        Some(BlueStoreOmapKeyFamily::PerPg)
    );
}

#[test]
fn rejects_trailing_value_bytes_and_memory_limit() {
    let mut fragment = BlueStoreOmapFragment::default();
    fragment
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(NID, flags(true, false, false, false)),
        )
        .expect("owner");
    let header = logical_key(BlueStoreOmapKeyFamily::Bulk, NID, b'-', &[]);
    fragment
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::Bulk, &header, &[])
        .expect("header");
    let mut value = encoded(1_u64);
    value.push(0);
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'.',
        b"size",
        &value,
    )
    .expect("deferred size");
    observe(
        &mut fragment,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'~',
        &[],
        &[],
    )
    .expect("tail");
    assert!(matches!(
        fragment.finish(),
        Err(BlueStoreOmapError::TrailingValue {
            field: "rbd header size",
            ..
        })
    ));

    let mut limited = BlueStoreOmapFragment::new(BlueStoreOmapLimits {
        max_scopes: 1,
        max_entries_per_scope: 0,
        max_owners: 1,
        max_retained_text_bytes: 1,
    });
    limited
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::Bulk, &header, &[])
        .expect("limited header");
    assert!(matches!(
        observe(
            &mut limited,
            BlueStoreOmapKeyFamily::Bulk,
            NID,
            b'.',
            b"size",
            &encoded(1_u64),
        ),
        Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP entries",
            ..
        })
    ));
}

#[test]
fn rejects_invalid_rbd_text_and_duplicate_fields() {
    let mut invalid = BlueStoreOmapFragment::default();
    invalid
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(NID, flags(true, false, false, false)),
        )
        .expect("owner");
    let header = logical_key(BlueStoreOmapKeyFamily::Bulk, NID, b'-', &[]);
    invalid
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::Bulk, &header, &[])
        .expect("header");
    let invalid_text = encoded_string("");
    observe(
        &mut invalid,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'.',
        b"object_prefix",
        &invalid_text,
    )
    .expect("deferred object prefix");
    observe(
        &mut invalid,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'~',
        &[],
        &[],
    )
    .expect("tail");
    assert!(matches!(
        invalid.finish(),
        Err(BlueStoreOmapError::InvalidField {
            field: "rbd header object_prefix",
            ..
        })
    ));

    let mut duplicate = BlueStoreOmapFragment::default();
    duplicate
        .observe_onode(
            &object(b"rbd_header.image-id"),
            &onode(NID, flags(true, false, false, false)),
        )
        .expect("owner");
    duplicate
        .observe_routed_latest_value(BlueStoreOmapKeyFamily::Bulk, &header, &[])
        .expect("header");
    observe(
        &mut duplicate,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'.',
        b"size",
        &encoded(1_u64),
    )
    .expect("size");
    observe(
        &mut duplicate,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'.',
        b"size",
        &encoded(2_u64),
    )
    .expect("duplicate size deferred");
    observe(
        &mut duplicate,
        BlueStoreOmapKeyFamily::Bulk,
        NID,
        b'~',
        &[],
        &[],
    )
    .expect("tail");
    assert!(matches!(
        duplicate.finish(),
        Err(BlueStoreOmapError::DuplicateField {
            field: "rbd header size",
            ..
        })
    ));
}
