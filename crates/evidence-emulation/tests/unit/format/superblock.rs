use super::*;

#[test]
fn superblock_encode_writes_magic_version_geometry_and_checksum() {
    let fingerprint = [0x5au8; 32];
    let parent = ParentIdentity::new(1_048_576, fingerprint).unwrap();
    let header = Superblock::new(parent, 4096);
    let bytes = header.encode();

    assert_eq!(bytes.len(), HEADER_SIZE);
    assert_eq!(&bytes[..8], MAGIC);
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        VERSION
    );
    assert_eq!(
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        HEADER_SIZE as u32
    );
    assert_eq!(
        u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        1_048_576
    );
    assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 4096);
    assert_eq!(
        u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
        DATA_START
    );
    assert_eq!(&bytes[64..96], &fingerprint);

    let checksum = u32::from_le_bytes(bytes[CHECKSUM_OFFSET..].try_into().unwrap());
    assert_eq!(checksum, crc32c::checksum(&bytes[..CHECKSUM_OFFSET]));
    assert_ne!(checksum, 0);
}
