//! A volume with a `compat_ro` feature this driver does not maintain
//! mounts read-only and refuses to mount read-write (#72).
//!
//! `mount_rw` consulted `compat_ro_flags` for nothing, and this driver
//! writes: a block group tree volume would be written by code that looks
//! for block group items in the extent tree and finds none. The image is
//! made fresh by `mkfs.btrfs` on a plain file -- no mount, no root. The
//! tool runs in the harness VM, which is where this suite's btrfs-progs
//! lives, so it is always there: a run either has the VM or fails saying
//! so, and nothing here reads as a pass for want of a tool.

use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::stamp_checksum;
use fs_btrfs::superblock::{compat_ro, ChecksumType};
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockDevice, BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
const COMPAT_RO_FLAGS: usize = 0xb4;

/// A fresh image with `bits` OR-ed into the primary superblock's
/// `compat_ro_flags`.
///
/// It is written under the suite's scratch directory inside this
/// repository: the guest running `mkfs.btrfs` sees this tree and nothing
/// else of the host.
fn image_with_compat_ro(name: &str, bits: u64) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("compat-ro-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("img");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs").arg("-f").arg(&path).output();
    assert!(
        made.status.success(),
        "mkfs.btrfs failed: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    let mut bytes = std::fs::read(&path).unwrap();
    let sb = &mut bytes[SUPERBLOCK..SUPERBLOCK + 4096];
    let flags = u64::from_le_bytes(sb[COMPAT_RO_FLAGS..COMPAT_RO_FLAGS + 8].try_into().unwrap());
    sb[COMPAT_RO_FLAGS..COMPAT_RO_FLAGS + 8].copy_from_slice(&(flags | bits).to_le_bytes());
    stamp_checksum(sb, ChecksumType::Crc32c);
    std::fs::write(&path, &bytes).unwrap();
    path
}

#[test]
fn an_unmaintained_compat_ro_feature_mounts_read_only_and_refuses_read_write() {
    for (name, bits) in [
        ("bgt", compat_ro::BLOCK_GROUP_TREE),
        ("verity", compat_ro::VERITY),
        ("future", 1 << 40),
    ] {
        let path = image_with_compat_ro(name, bits);
        let ro = FileDevice::open(&path).unwrap();
        Filesystem::mount(Arc::new(ro) as Arc<dyn BlockRead>)
            .unwrap_or_else(|e| panic!("{name}: a compat_ro bit must not stop a read: {e:?}"));

        let rw = FileDevice::open_rw(&path).unwrap();
        match Filesystem::mount_rw(Arc::new(rw) as Arc<dyn BlockDevice>) {
            Err(fs_btrfs::error::Error::UnsupportedFeature(m)) => {
                assert!(m.contains("can be read but not written"), "{name}: {m}")
            }
            Err(other) => panic!("{name}: refused for the wrong reason: {other:?}"),
            Ok(_) => panic!("{name}: mounted read-write over a feature it does not maintain"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

/// The control: a fresh `mkfs.btrfs` image, whose free-space tree bits
/// this driver maintains, still mounts read-write.
#[test]
fn a_fresh_image_still_mounts_read_write() {
    let path = image_with_compat_ro("fresh", 0);
    let rw = FileDevice::open_rw(&path).unwrap();
    Filesystem::mount_rw(Arc::new(rw) as Arc<dyn BlockDevice>)
        .unwrap_or_else(|e| panic!("a fresh image must mount read-write: {e:?}"));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
