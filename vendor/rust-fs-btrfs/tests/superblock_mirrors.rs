//! The superblock is read from every copy, and a mount uses the newest
//! valid one (#90).
//!
//! Every read hardcoded copy 0, so a volume whose primary superblock was
//! damaged could not be mounted though the commit path writes all three.
//! A fresh `mkfs.btrfs` image of 256 MiB has copies 0 and 1 and no copy 2,
//! so the absent copy is exercised by every case. `mkfs.btrfs` runs in the
//! harness VM, which is the one place the btrfs-progs tools live, so it is
//! always there and nothing here skips.

use fs_btrfs::error::Error;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::stamp_checksum;
use fs_btrfs::superblock::{offsets, read_superblock, ChecksumType, SUPER_OFFSETS};
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockDevice, BlockRead, FileDevice};
use std::sync::Arc;

/// A fresh volume of 256 MiB, under the suite's own directory inside this
/// repository -- which is where it has to be, because the guest that runs
/// `mkfs.btrfs` sees this repository and nothing else of the host.
fn fresh_image(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("sb-mirrors-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("img");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs").arg("-f").arg(&path).output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    path
}

/// Rewrite one superblock copy in place: 4 KiB read and written, not
/// the whole image.
fn edit_copy(path: &std::path::Path, copy: usize, edit: impl FnOnce(&mut [u8])) {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut bytes = [0u8; 4096];
    file.seek(SeekFrom::Start(SUPER_OFFSETS[copy])).unwrap();
    file.read_exact(&mut bytes).unwrap();
    edit(&mut bytes);
    file.seek(SeekFrom::Start(SUPER_OFFSETS[copy])).unwrap();
    file.write_all(&bytes).unwrap();
}

fn read(path: &std::path::Path) -> (u64, usize) {
    let dev = FileDevice::open(path).unwrap();
    let (sb, copy) = read_superblock(&dev).expect("a valid copy");
    (sb.generation, copy)
}

fn mount_ro(path: &std::path::Path) -> Result<Filesystem, Error> {
    Filesystem::mount(Arc::new(FileDevice::open(path).unwrap()) as Arc<dyn BlockRead>)
}

fn mount_rw(path: &std::path::Path) -> Result<Filesystem, Error> {
    Filesystem::mount_rw(Arc::new(FileDevice::open_rw(path).unwrap()) as Arc<dyn BlockDevice>)
}

/// Control: an undamaged volume reads copy 0, and the absent copy 2 is
/// not an error for either mount.
#[test]
fn an_undamaged_volume_uses_the_primary_and_its_absent_third_copy_is_not_damage() {
    let path = fresh_image("control");
    assert_eq!(read(&path).1, 0);
    mount_ro(&path).expect("mounts read-only");
    mount_rw(&path).expect("mounts read-write");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// A zeroed primary mounts from copy 1, read-only, and refuses to be
/// written until it has been checked.
#[test]
fn a_zeroed_primary_mounts_read_only_from_the_mirror() {
    let path = fresh_image("zeroed");
    edit_copy(&path, 0, |sb| sb.fill(0));
    assert_eq!(read(&path).1, 1, "the mirror was not used");
    mount_ro(&path).unwrap_or_else(|e| panic!("a zeroed primary must mount from copy 1: {e:?}"));
    match mount_rw(&path) {
        Err(Error::UnsupportedFeature(m)) => assert!(m.contains("copy 1"), "{m}"),
        other => panic!(
            "a mirror mount was allowed to write: {:?}",
            other.map(|_| ())
        ),
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// Two valid copies that disagree: the newer generation wins, which is
/// what a commit torn between its copies needs.
#[test]
fn the_copy_with_the_newer_generation_wins() {
    let path = fresh_image("newer");
    let (gen0, _) = read(&path);
    edit_copy(&path, 1, |sb| {
        sb[offsets::GENERATION..offsets::GENERATION + 8].copy_from_slice(&(gen0 + 1).to_le_bytes());
        stamp_checksum(sb, ChecksumType::Crc32c);
    });
    assert_eq!(
        read(&path),
        (gen0 + 1, 1),
        "the older primary was preferred"
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// A file device whose first read of superblock copy 1 fails and whose
/// later reads succeed.
struct MirrorFailsOnce {
    inner: FileDevice,
    failed: std::sync::atomic::AtomicBool,
}

impl BlockRead for MirrorFailsOnce {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> fs_core::Result<()> {
        if offset == SUPER_OFFSETS[1]
            && !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(fs_core::Error::OutOfBounds {
                offset,
                len: buf.len() as u64,
                size: 0,
            });
        }
        self.inner.read_at(offset, buf)
    }
    fn size_bytes(&self) -> u64 {
        BlockRead::size_bytes(&self.inner)
    }
}

impl BlockDevice for MirrorFailsOnce {
    fn write_at(&self, offset: u64, buf: &[u8]) -> fs_core::Result<()> {
        self.inner.write_at(offset, buf)
    }
    fn flush(&self) -> fs_core::Result<()> {
        self.inner.flush()
    }
    fn is_writable(&self) -> bool {
        true
    }
}

/// The read-write refusal is made on the superblock the mount uses.
///
/// Copy 1 is newer, and its first read fails. A refusal checked on its own
/// read beforehand saw only copy 0 and let the mount through, and the
/// mount's own selection then read copy 1 and mounted it writable
/// (Greptile on #146). A writable mount must never be of a mirror.
#[test]
fn a_mirror_that_reads_only_the_second_time_is_not_mounted_writable() {
    let path = fresh_image("flaky");
    let (gen0, _) = read(&path);
    edit_copy(&path, 1, |sb| {
        sb[offsets::GENERATION..offsets::GENERATION + 8].copy_from_slice(&(gen0 + 1).to_le_bytes());
        stamp_checksum(sb, ChecksumType::Crc32c);
    });
    let dev = Arc::new(MirrorFailsOnce {
        inner: FileDevice::open_rw(&path).unwrap(),
        failed: std::sync::atomic::AtomicBool::new(false),
    });
    if let Ok(fs) = Filesystem::mount_rw(dev as Arc<dyn BlockDevice>) {
        assert_eq!(
            fs.superblock().generation,
            gen0,
            "mounted writable from the mirror"
        );
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
