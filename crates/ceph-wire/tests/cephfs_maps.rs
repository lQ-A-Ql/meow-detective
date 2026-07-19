use ceph_wire::{decode_ceph_fs_map, decode_ceph_mds_map, CephEncode, CephMdsState, CephWireError};

#[derive(Clone, Copy)]
struct MdsFixture {
    gid: u64,
    rank: i32,
    state: i32,
}

fn append<T: CephEncode>(output: &mut Vec<u8>, value: T) {
    value.encode(output);
}

fn envelope(version: u8, compat: u8, payload: Vec<u8>) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(6 + payload.len());
    append(&mut encoded, version);
    append(&mut encoded, compat);
    append(&mut encoded, payload.len() as u32);
    encoded.extend_from_slice(&payload);
    encoded
}

fn append_empty_compat_set(output: &mut Vec<u8>) {
    for _ in 0..3 {
        append(output, 0u64);
        append(output, 0u32);
    }
}

fn encode_mds_info(fixture: MdsFixture) -> Vec<u8> {
    let mut payload = Vec::new();
    append(&mut payload, fixture.gid);
    append(&mut payload, format!("mds-{}", fixture.gid));
    append(&mut payload, fixture.rank);
    append(&mut payload, 4i32);
    append(&mut payload, fixture.state);
    append(&mut payload, 99u64);
    append(&mut payload, 2u8); // entity_addrvec marker
    append(&mut payload, 0u32); // empty address vector
    append(&mut payload, 0u32); // laggy seconds
    append(&mut payload, 0u32); // laggy nanoseconds
    append(&mut payload, -1i32); // standby_for_rank
    append(&mut payload, String::new()); // standby_for_name
    append_i32_set(&mut payload, &[]); // export targets
    append(&mut payload, 0u64); // MDS features
    append(&mut payload, -1i64); // join filesystem ID
    append(&mut payload, 0u8); // standby replay
    append(&mut payload, 0u64); // flags
    append_empty_compat_set(&mut payload);
    envelope(10, 4, payload)
}

fn append_i32_set(output: &mut Vec<u8>, values: &[i32]) {
    append(output, values.len() as u32);
    for value in values {
        append(output, *value);
    }
}

fn encode_mds_map(
    epoch: u32,
    name: &str,
    metadata_pool: i64,
    data_pools: &[i64],
    daemon: MdsFixture,
) -> Vec<u8> {
    let mut payload = Vec::new();
    append(&mut payload, epoch);
    append(&mut payload, 0u32); // flags
    append(&mut payload, 0u32); // last failure
    append(&mut payload, 0i32); // root rank
    append(&mut payload, 60u32);
    append(&mut payload, 300u32);
    append(&mut payload, 1u64 << 40);
    append(&mut payload, 1i32); // max_mds
    append(&mut payload, 1u32); // mds_info count
    append(&mut payload, daemon.gid);
    payload.extend_from_slice(&encode_mds_info(daemon));
    append(&mut payload, data_pools.len() as u32);
    for pool in data_pools {
        append(&mut payload, *pool);
    }
    append(&mut payload, -1i64); // cas_pool
    append(&mut payload, 19u16); // extended encoding version
    append_empty_compat_set(&mut payload);
    append(&mut payload, metadata_pool);
    append(&mut payload, 1u32); // created seconds
    append(&mut payload, 0u32); // created nanoseconds
    append(&mut payload, 2u32); // modified seconds
    append(&mut payload, 0u32); // modified nanoseconds
    append(&mut payload, 0i32); // tableserver
    append_i32_set(&mut payload, &[daemon.rank]);
    append(&mut payload, 1u32); // rank incarnation map
    append(&mut payload, daemon.rank);
    append(&mut payload, 4i32);
    append(&mut payload, 1u32); // up map
    append(&mut payload, daemon.rank);
    append(&mut payload, daemon.gid);
    append_i32_set(&mut payload, &[]); // failed
    append_i32_set(&mut payload, &[]); // stopped
    append(&mut payload, 77u32); // last failure OSD epoch
    append(&mut payload, 0u8); // ever allowed snapshots
    append(&mut payload, 0u8); // explicitly allowed snapshots
    append(&mut payload, 1u8); // inline data enabled
    append(&mut payload, 1u8); // enabled
    append(&mut payload, name.to_string());
    append_i32_set(&mut payload, &[]); // damaged
    append(&mut payload, String::new()); // balancer
    append(&mut payload, -1i32); // standby count wanted
    append(&mut payload, 1i32); // old max MDS
    append(&mut payload, 0u8); // minimum compatible client release
    append(&mut payload, 0u32); // required client feature bytes
    append(&mut payload, "-1".to_string()); // balancer rank mask
    append(&mut payload, 64u64 * 1024); // max xattr size
    append(&mut payload, 0u64); // quiesce DB leader
    append(&mut payload, 0u32); // quiesce DB members
    envelope(5, 4, payload)
}

fn encode_filesystem(filesystem_id: i64, mds_map: Vec<u8>) -> Vec<u8> {
    let mut payload = Vec::new();
    append(&mut payload, filesystem_id);
    append(&mut payload, mds_map.len() as u32);
    payload.extend_from_slice(&mds_map);

    let mut mirror_payload = Vec::new();
    append(&mut mirror_payload, 0u8); // mirrored
    append(&mut mirror_payload, 0u32); // peer count
    payload.extend_from_slice(&envelope(1, 1, mirror_payload));
    envelope(2, 1, payload)
}

fn encode_fs_map(epoch: u32, filesystems: Vec<Vec<u8>>) -> Vec<u8> {
    let mut payload = Vec::new();
    append(&mut payload, epoch);
    append(&mut payload, 100i64); // next filesystem ID
    append(&mut payload, -1i64); // legacy filesystem ID
    append_empty_compat_set(&mut payload);
    append(&mut payload, 1u8); // multiple filesystems enabled
    append(&mut payload, filesystems.len() as u32);
    for filesystem in filesystems {
        payload.extend_from_slice(&filesystem);
    }
    append(&mut payload, 0u32); // MDS roles
    append(&mut payload, 0u32); // standby daemons
    append(&mut payload, 0u32); // standby epochs
    append(&mut payload, 1u8); // ever enabled multiple
    append(&mut payload, 1u32); // birth time seconds
    append(&mut payload, 0u32); // birth time nanoseconds
    envelope(8, 6, payload)
}

fn active_daemon() -> MdsFixture {
    MdsFixture {
        gid: 123,
        rank: 0,
        state: 13,
    }
}

#[test]
fn decodes_fsmap_filesystem_pool_and_mds_identity() {
    let mds_map = encode_mds_map(17, "cephfs-a", 3, &[5, 7], active_daemon());
    let encoded = encode_fs_map(17, vec![encode_filesystem(1, mds_map)]);

    let decoded = decode_ceph_fs_map(&encoded).expect("decode FSMap");
    assert_eq!(decoded.epoch, 17);
    assert_eq!(decoded.filesystems.len(), 1);
    let filesystem = &decoded.filesystems[0];
    assert_eq!(filesystem.filesystem_id, 1);
    assert_eq!(filesystem.mds_map.epoch, 17);
    assert_eq!(filesystem.mds_map.name, "cephfs-a");
    assert_eq!(filesystem.mds_map.metadata_pool_id, 3);
    assert_eq!(filesystem.mds_map.data_pool_ids, vec![5, 7]);
    assert_eq!(filesystem.mds_map.last_failure_osd_epoch, 77);
    assert_eq!(filesystem.mds_map.up_ranks.get(&0), Some(&123));
    assert_eq!(filesystem.mds_map.daemons[0].state, CephMdsState::Active);
    assert_eq!(filesystem.mds_map.daemons[0].incarnation, 4);
    assert_eq!(filesystem.mds_map.daemons[0].state_sequence, 99);
}

#[test]
fn standalone_mdsmap_decoder_preserves_recovery_state() {
    let daemon = MdsFixture {
        state: 8,
        ..active_daemon()
    };
    let decoded = decode_ceph_mds_map(&encode_mds_map(9, "recovery", 2, &[4], daemon))
        .expect("decode MDSMap");
    assert_eq!(decoded.daemons[0].state, CephMdsState::Replay);
    assert!(!decoded.daemons[0].state.is_active());
}

#[test]
fn rejects_every_truncated_fsmap_prefix() {
    let mds_map = encode_mds_map(17, "cephfs-a", 3, &[5], active_daemon());
    let encoded = encode_fs_map(17, vec![encode_filesystem(1, mds_map)]);
    for length in 0..encoded.len() {
        assert!(
            decode_ceph_fs_map(&encoded[..length]).is_err(),
            "truncated prefix {length} unexpectedly decoded"
        );
    }
}

#[test]
fn rejects_duplicate_filesystems_and_inconsistent_nested_epoch() {
    let mds_map = encode_mds_map(17, "cephfs-a", 3, &[5], active_daemon());
    let duplicate = encode_fs_map(
        17,
        vec![
            encode_filesystem(1, mds_map.clone()),
            encode_filesystem(1, mds_map),
        ],
    );
    assert!(matches!(
        decode_ceph_fs_map(&duplicate),
        Err(CephWireError::DuplicateCephFsIdentifier {
            kind: "filesystem",
            value: 1
        })
    ));

    let inconsistent = encode_fs_map(
        18,
        vec![encode_filesystem(
            1,
            encode_mds_map(17, "cephfs-a", 3, &[5], active_daemon()),
        )],
    );
    assert!(matches!(
        decode_ceph_fs_map(&inconsistent),
        Err(CephWireError::InvalidCephFsMap {
            field: "filesystem.mds_map.epoch",
            ..
        })
    ));
}

#[test]
fn rejects_unknown_state_invalid_pool_and_trailing_bytes() {
    let unknown = MdsFixture {
        state: 99,
        ..active_daemon()
    };
    assert_eq!(
        decode_ceph_mds_map(&encode_mds_map(7, "cephfs", 3, &[5], unknown)).unwrap_err(),
        CephWireError::UnknownCephFsMdsState { value: 99 }
    );

    assert!(matches!(
        decode_ceph_mds_map(&encode_mds_map(7, "cephfs", 0, &[5], active_daemon())),
        Err(CephWireError::InvalidCephFsMap {
            field: "metadata_pool",
            ..
        })
    ));

    let mut trailing = encode_mds_map(7, "cephfs", 3, &[5], active_daemon());
    trailing.push(0);
    assert_eq!(
        decode_ceph_mds_map(&trailing).unwrap_err(),
        CephWireError::CephFsTrailingBytes {
            map: "MDSMap",
            remaining: 1
        }
    );
}

#[test]
fn rejects_future_fsmap_versions_until_the_layout_is_reviewed() {
    let mds_map = encode_mds_map(17, "cephfs-a", 3, &[5], active_daemon());
    let mut encoded = encode_fs_map(17, vec![encode_filesystem(1, mds_map)]);
    encoded[0] = 9;
    assert_eq!(
        decode_ceph_fs_map(&encoded).unwrap_err(),
        CephWireError::UnsupportedCephFsMapVersion {
            map: "FSMap",
            encoded_version: 9,
            minimum_version: 7,
            decoder_version: 8,
        }
    );
}

#[test]
fn state_mapping_covers_all_ceph_mds_wire_states() {
    for value in [
        -10, -9, -8, -7, -6, -5, -4, -1, 0, 8, 9, 10, 11, 12, 13, 14, 15,
    ] {
        CephMdsState::from_raw(value).expect("known Ceph MDS state");
    }
    assert_eq!(
        CephMdsState::from_raw(7).unwrap_err(),
        CephWireError::UnknownCephFsMdsState { value: 7 }
    );
}
