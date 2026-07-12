use super::*;

#[test]
fn mft_inode_fast_path_handles_partition_record_format() {
    // "mft:3:42" format from parallel MFT enumeration
    let path = "mft:3:42";
    let inode = mft_inode_from_path(path);
    assert_eq!(inode, Some(42));
}

#[test]
fn mft_inode_fast_path_handles_legacy_format() {
    // "mft:5" format from legacy MFT enumeration
    let path = "mft:5";
    let inode = mft_inode_from_path(path);
    assert_eq!(inode, Some(5));
}
