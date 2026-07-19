use ceph_wire::{
    decode_cephfs_dentry_key, decode_cephfs_dentry_value, CephFsDentryKind, CephWireError,
    CEPH_NOSNAP,
};

#[test]
fn decodes_canonical_dirfrag_and_dentry_forms() {
    let identity = ceph_wire::CephFsDirfragIdentity::parse_object_name("2.0000000a").unwrap();
    assert_eq!(identity.inode, 2);
    assert_eq!(identity.fragment, 10);
    assert_eq!(identity.object_name(), "2.0000000a");

    let key = decode_cephfs_dentry_key(b"child_head").unwrap();
    assert_eq!(key.name, "child");
    assert_eq!(key.snap_id, CEPH_NOSNAP);
    let value = primary_value(42);
    let dentry = decode_cephfs_dentry_value(key, &value).unwrap();
    assert!(matches!(dentry.kind, CephFsDentryKind::Primary));
    assert_eq!(dentry.child_inode, 42);
    assert_eq!(dentry.inode.unwrap().size, 99);

    let remote = decode_cephfs_dentry_value(
        decode_cephfs_dentry_key(b"remote_head").unwrap(),
        &remote_value(b'L', 77, 4),
    )
    .unwrap();
    assert!(remote.kind.is_directory_hint());
    assert_eq!(remote.child_inode, 77);
}

#[test]
fn rejects_unsafe_keys_and_unknown_types() {
    assert!(decode_cephfs_dentry_key(b"../head").is_err());
    assert!(decode_cephfs_dentry_key(b"name_01").is_err());
    assert!(matches!(
        decode_cephfs_dentry_value(
            decode_cephfs_dentry_key(b"x_head").unwrap(),
            &remote_value(b'?', 1, 0)
        ),
        Err(CephWireError::UnsupportedCephFsDentryType { value: b'?' })
    ));
}

fn primary_value(ino: u64) -> Vec<u8> {
    let mut output = 5u64.to_le_bytes().to_vec();
    output.push(b'I');
    output.extend(inode_envelope(&inode_payload(ino, 0o100000 | 0o640, 99)));
    output
}

fn remote_value(kind: u8, ino: u64, d_type: u8) -> Vec<u8> {
    let mut output = 5u64.to_le_bytes().to_vec();
    output.push(kind);
    output.extend(ino.to_le_bytes());
    output.push(d_type);
    output
}

fn inode_payload(ino: u64, mode: u32, size: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend(ino.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend([0u8; 8]);
    payload.extend(mode.to_le_bytes());
    payload.extend(1000u32.to_le_bytes());
    payload.extend(1000u32.to_le_bytes());
    payload.extend(1i32.to_le_bytes());
    payload.push(0);
    payload.extend([0u8; 8]);
    for value in [64 * 1024, 1, 64 * 1024, 0, 0, 0, 2] {
        payload.extend((value as u32).to_le_bytes());
    }
    payload.extend(size.to_le_bytes());
    payload
}

fn inode_envelope(payload: &[u8]) -> Vec<u8> {
    let mut output = vec![20, 6];
    output.extend((payload.len() as u32).to_le_bytes());
    output.extend(payload);
    output
}
