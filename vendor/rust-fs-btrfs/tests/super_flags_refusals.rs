//! The superblock `flags` this driver must act on are acted on (#76).
//!
//! `is_metadump` and `is_seeding` were defined and called by nothing. A
//! metadata-only dump mounted and read garbage from where its absent data
//! extents pointed, and a seed device mounted read-write. The images are
//! fresh `mkfs.btrfs` files with bits OR-ed into the primary superblock's
//! `flags`. `mkfs.btrfs` runs in the harness VM, which is the one place the
//! btrfs-progs tools live, so it is always there and nothing here skips.

use fs_btrfs::error::Error;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::stamp_checksum;
use fs_btrfs::superblock::{offsets, super_flags, ChecksumType};
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockDevice, BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;

/// Removes the scratch directory however the test ends.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scratch directory of this case's own, below the suite's own
/// directory inside this repository -- which is where it has to be,
/// because the guest that runs `mkfs.btrfs` sees this repository and
/// nothing else of the host.
///
/// `create_dir`, not `create_dir_all`: the suite's directory is this
/// process's alone, so a name already taken means two cases share one
/// image, and stopping says so where a silent reuse would not.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("sb-flags-{tag}"));
    std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

/// A fresh image with `bits` OR-ed into the superblock's `flags`.
fn image_with_flags(name: &str, bits: u64) -> (Scratch, std::path::PathBuf) {
    let dir = scratch_dir(name);
    let scratch = Scratch(dir.clone());
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
    let at = offsets::FLAGS;
    let flags = u64::from_le_bytes(sb[at..at + 8].try_into().unwrap());
    sb[at..at + 8].copy_from_slice(&(flags | bits).to_le_bytes());
    stamp_checksum(sb, ChecksumType::Crc32c);
    std::fs::write(&path, &bytes).unwrap();
    (scratch, path)
}

fn mount(path: &std::path::Path) -> fs_btrfs::error::Result<Filesystem> {
    Filesystem::mount(Arc::new(FileDevice::open(path).unwrap()) as Arc<dyn BlockRead>)
}

fn mount_rw(path: &std::path::Path) -> fs_btrfs::error::Result<Filesystem> {
    Filesystem::mount_rw(Arc::new(FileDevice::open_rw(path).unwrap()) as Arc<dyn BlockDevice>)
}

#[test]
fn a_metadata_only_dump_is_refused_at_every_mount() {
    for (name, bits) in [
        ("metadump", super_flags::METADUMP),
        ("metadump-v2", super_flags::METADUMP_V2),
    ] {
        let (_scratch, path) = image_with_flags(name, bits);
        for (how, result) in [("read-only", mount(&path)), ("read-write", mount_rw(&path))] {
            match result {
                Err(Error::UnsupportedFeature(m)) => {
                    assert!(m.contains("metadata-only dump"), "{name} {how}: {m}")
                }
                Err(other) => panic!("{name} {how}: refused for the wrong reason: {other:?}"),
                Ok(_) => panic!("{name}: a metadata-only dump mounted {how}"),
            }
        }
    }
}

#[test]
fn a_seed_device_mounts_read_only_and_refuses_read_write() {
    let (_scratch, path) = image_with_flags("seed", super_flags::SEEDING);
    mount(&path).unwrap_or_else(|e| panic!("a seed must still be readable: {e:?}"));
    match mount_rw(&path) {
        Err(Error::UnsupportedFeature(m)) => assert!(m.contains("seed"), "{m}"),
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a seed device mounted read-write"),
    }
}

/// The control: the same image with no flag added mounts both ways, and
/// the error flag, which gates nothing, does not stop either.
#[test]
fn an_unflagged_image_and_the_error_flag_still_mount_read_write() {
    for (name, bits) in [("plain", 0), ("error-flag", super_flags::ERROR)] {
        let (_scratch, path) = image_with_flags(name, bits);
        mount(&path).unwrap_or_else(|e| panic!("{name}: read-only: {e:?}"));
        mount_rw(&path).unwrap_or_else(|e| panic!("{name}: read-write: {e:?}"));
    }
}
