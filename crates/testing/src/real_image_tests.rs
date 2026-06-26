//! Real-image regression test harness.
//!
//! To run: set `FORENSICS_REAL_IMAGE_DIR` to a directory containing
//! real disk images (E01, dd, raw) and run:
//!   cargo test --features real-image-tests -- --ignored
//!
//! Required test images per filesystem:
//!   - NTFS: ntfs_sample.E01 or ntfs_sample.dd
//!   - FAT32: fat32_sample.dd
//!   - exFAT: exfat_sample.dd
//!   - ext4: ext4_sample.dd
//!   - XFS: xfs_sample.dd
//!   - Btrfs: btrfs_sample.dd
//!   - APFS: apfs_sample.dmg
//!   - HFS+: hfsplus_sample.dmg
