use ceph_wire::{
    decode_cephfs_inode_object, decode_cephfs_inode_store, decode_cephfs_inode_t_prefix,
    CephFsInodeKind, CephWireError, S_IFDIR, S_IFREG,
};

#[test]
fn decodes_inode_prefix_store_and_object_envelopes() {
    let inode = inode_payload(42, S_IFREG | 0o640, 99);
    let prefix = inode_envelope(&inode);
    let projection = decode_cephfs_inode_t_prefix(&prefix).unwrap();
    assert_eq!(projection.ino, 42);
    assert_eq!(projection.kind, CephFsInodeKind::File);
    assert_eq!(projection.size, 99);

    let store = envelope(6, 4, &prefix);
    assert_eq!(decode_cephfs_inode_store(&store).unwrap().ino, 42);

    let mut object = Vec::new();
    put_string(&mut object, "ceph fs volume v011");
    object.extend(store);
    assert_eq!(decode_cephfs_inode_object(&object).unwrap().ino, 42);
}

#[test]
fn classifies_directory_and_rejects_bad_inode_headers() {
    let prefix = inode_envelope(&inode_payload(1, S_IFDIR | 0o755, 0));
    assert!(decode_cephfs_inode_t_prefix(&prefix)
        .unwrap()
        .is_directory());

    let mut bad_magic = Vec::new();
    put_string(&mut bad_magic, "not cephfs");
    assert!(matches!(
        decode_cephfs_inode_object(&bad_magic),
        Err(CephWireError::InvalidCephFsInode { field: "magic", .. })
    ));

    let mut truncated = prefix;
    truncated.truncate(truncated.len() - 1);
    assert!(matches!(
        decode_cephfs_inode_t_prefix(&truncated),
        Err(CephWireError::UnexpectedEof { .. })
    ));
}

fn inode_payload(ino: u64, mode: u32, size: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend(ino.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend(mode.to_le_bytes());
    payload.extend(1000u32.to_le_bytes());
    payload.extend(1000u32.to_le_bytes());
    payload.extend(1i32.to_le_bytes());
    payload.push(0);
    payload.extend([0u8; 8]);
    payload.extend(legacy_layout(64 * 1024, 1, 64 * 1024, 2));
    payload.extend(size.to_le_bytes());
    payload
}

fn inode_envelope(payload: &[u8]) -> Vec<u8> {
    envelope(20, 6, payload)
}

fn envelope(version: u8, compat: u8, payload: &[u8]) -> Vec<u8> {
    let mut output = vec![version, compat];
    output.extend((payload.len() as u32).to_le_bytes());
    output.extend(payload);
    output
}

fn legacy_layout(stripe_unit: u32, stripe_count: u32, object_size: u32, pool: u32) -> Vec<u8> {
    let mut output = Vec::new();
    for value in [stripe_unit, stripe_count, object_size, 0, 0, 0, pool] {
        output.extend(value.to_le_bytes());
    }
    output
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    output.extend((value.len() as u32).to_le_bytes());
    output.extend(value.as_bytes());
}
