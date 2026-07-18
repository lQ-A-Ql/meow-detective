use super::*;
use crate::reader::{S_IFDIR, XFS_INODE_MAGIC};

fn inode(version: u8) -> Vec<u8> {
    let size = if version == 3 {
        INODE_CORE_SIZE_V3
    } else {
        INODE_CORE_SIZE
    };
    let mut inode = vec![0u8; size];
    inode[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    inode[di_off::VERSION] = version;
    inode[di_off::FORMAT] = FORMAT_EXTENTS;
    inode
}

fn write_legacy_timestamp(inode: &mut [u8], offset: usize, seconds: i32, nanoseconds: u32) {
    inode[offset..offset + 4].copy_from_slice(&seconds.to_be_bytes());
    inode[offset + 4..offset + 8].copy_from_slice(&nanoseconds.to_be_bytes());
}

fn write_bigtime_timestamp(inode: &mut [u8], offset: usize, unix_seconds: i64, nanoseconds: u32) {
    let ondisk_seconds = unix_seconds
        .checked_add(XFS_BIGTIME_EPOCH_OFFSET)
        .and_then(|value| u64::try_from(value).ok())
        .unwrap();
    let total_nanoseconds = ondisk_seconds
        .checked_mul(NSEC_PER_SEC)
        .and_then(|value| value.checked_add(u64::from(nanoseconds)))
        .unwrap();
    inode[offset..offset + 8].copy_from_slice(&total_nanoseconds.to_be_bytes());
}

fn assert_timestamp(timestamp: Option<FsTimestamp>, seconds: i64, nanoseconds: u32) {
    let timestamp = timestamp.expect("timestamp");
    assert_eq!(timestamp.timestamp(), seconds);
    assert_eq!(timestamp.timestamp_subsec_nanos(), nanoseconds);
}

#[test]
fn v2_inode_decodes_signed_legacy_macb_timestamps() {
    let mut inode = inode(2);
    inode[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    inode[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&4096u64.to_be_bytes());
    write_legacy_timestamp(&mut inode, di_off::ATIME, -1, 123);
    write_legacy_timestamp(&mut inode, di_off::MTIME, 0, 456);
    write_legacy_timestamp(&mut inode, di_off::CTIME, i32::MAX, 999_999_999);

    let metadata = XfsReader::decode_inode_metadata(&inode).unwrap();

    assert!(metadata.is_dir);
    assert_eq!(metadata.size, 4096);
    assert!(metadata.created_at.is_none());
    assert_timestamp(metadata.accessed_at, -1, 123);
    assert_timestamp(metadata.modified_at, 0, 456);
    assert_timestamp(metadata.changed_at, i64::from(i32::MAX), 999_999_999);
}

#[test]
fn v3_inode_without_bigtime_decodes_legacy_crtime() {
    let mut inode = inode(3);
    inode[di_off::FLAGS2..di_off::FLAGS2 + 8].copy_from_slice(&(1u64 << 2).to_be_bytes());
    write_legacy_timestamp(&mut inode, di_off::ATIME, 10, 1);
    write_legacy_timestamp(&mut inode, di_off::MTIME, 20, 2);
    write_legacy_timestamp(&mut inode, di_off::CTIME, 30, 3);
    write_legacy_timestamp(&mut inode, di_off::CRTIME, 40, 4);

    let metadata = XfsReader::decode_inode_metadata(&inode).unwrap();

    assert_timestamp(metadata.accessed_at, 10, 1);
    assert_timestamp(metadata.modified_at, 20, 2);
    assert_timestamp(metadata.changed_at, 30, 3);
    assert_timestamp(metadata.created_at, 40, 4);
}

#[test]
fn v3_inode_with_bigtime_decodes_total_nanoseconds_and_epoch_offset() {
    let mut inode = inode(3);
    inode[di_off::FLAGS2..di_off::FLAGS2 + 8].copy_from_slice(&XFS_DIFLAG2_BIGTIME.to_be_bytes());
    write_bigtime_timestamp(&mut inode, di_off::ATIME, i64::from(i32::MIN), 0);
    write_bigtime_timestamp(&mut inode, di_off::MTIME, 0, 123_456_789);
    write_bigtime_timestamp(&mut inode, di_off::CTIME, 2_147_483_648, 999_999_999);
    write_bigtime_timestamp(&mut inode, di_off::CRTIME, 4_102_444_800, 42);

    let metadata = XfsReader::decode_inode_metadata(&inode).unwrap();

    assert_timestamp(metadata.accessed_at, i64::from(i32::MIN), 0);
    assert_timestamp(metadata.modified_at, 0, 123_456_789);
    assert_timestamp(metadata.changed_at, 2_147_483_648, 999_999_999);
    assert_timestamp(metadata.created_at, 4_102_444_800, 42);
}

#[test]
fn v2_inode_ignores_bigtime_bit_pattern_outside_the_v2_core() {
    let mut inode = vec![0u8; INODE_CORE_SIZE_V3];
    inode[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    inode[di_off::VERSION] = 2;
    inode[di_off::FORMAT] = FORMAT_EXTENTS;
    inode[di_off::FLAGS2..di_off::FLAGS2 + 8].copy_from_slice(&XFS_DIFLAG2_BIGTIME.to_be_bytes());
    write_legacy_timestamp(&mut inode, di_off::ATIME, -20, 20);
    write_legacy_timestamp(&mut inode, di_off::MTIME, -30, 30);
    write_legacy_timestamp(&mut inode, di_off::CTIME, -40, 40);

    let metadata = XfsReader::decode_inode_metadata(&inode).unwrap();

    assert_timestamp(metadata.accessed_at, -20, 20);
    assert_timestamp(metadata.modified_at, -30, 30);
    assert_timestamp(metadata.changed_at, -40, 40);
    assert!(metadata.created_at.is_none());
}

#[test]
fn legacy_timestamp_with_invalid_nanoseconds_fails_closed() {
    let mut inode = inode(2);
    write_legacy_timestamp(&mut inode, di_off::ATIME, 1, 1_000_000_000);

    let error = XfsReader::decode_inode_metadata(&inode).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("di_atime"));
    assert!(error.to_string().contains("invalid nanoseconds"));
}

#[test]
fn truncated_v2_and_v3_inode_cores_fail_closed() {
    for version in [2, 3] {
        let mut inode = inode(version);
        inode.pop();

        let error = XfsReader::decode_inode_metadata(&inode).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("inode core truncated"));
    }
}

#[test]
fn v1_inode_decodes_legacy_timestamps() {
    let mut inode = inode(1);
    write_legacy_timestamp(&mut inode, di_off::ATIME, 11, 1);
    write_legacy_timestamp(&mut inode, di_off::MTIME, 22, 2);
    write_legacy_timestamp(&mut inode, di_off::CTIME, 33, 3);

    let metadata = XfsReader::decode_inode_metadata(&inode).unwrap();

    assert_timestamp(metadata.accessed_at, 11, 1);
    assert_timestamp(metadata.modified_at, 22, 2);
    assert_timestamp(metadata.changed_at, 33, 3);
    assert!(metadata.created_at.is_none());
}

#[test]
fn unsupported_inode_version_fails_closed() {
    let inode = inode(4);

    let error = XfsReader::decode_inode_metadata(&inode).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .contains("unsupported XFS inode version 4"));
}

#[test]
fn bigtime_maximum_supported_second_is_valid() {
    let total_nanoseconds = XFS_BIGTIME_TIME_MAX
        .checked_mul(NSEC_PER_SEC)
        .and_then(|value| value.checked_add(NSEC_PER_SEC - 1))
        .unwrap();

    let timestamp = decode_bigtime(total_nanoseconds, "edge").unwrap();

    assert_timestamp(
        Some(timestamp),
        i64::try_from(XFS_BIGTIME_TIME_MAX).unwrap() - XFS_BIGTIME_EPOCH_OFFSET,
        999_999_999,
    );
}

#[test]
fn bigtime_second_beyond_xfs_limit_fails_closed() {
    let total_nanoseconds = (XFS_BIGTIME_TIME_MAX + 1)
        .checked_mul(NSEC_PER_SEC)
        .unwrap();

    let error = decode_bigtime(total_nanoseconds, "edge").unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .contains("exceeds supported XFS BIGTIME range"));
}
