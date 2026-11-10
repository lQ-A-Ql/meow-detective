//! A filesystem spanning two devices, opened with one.
//!
//! A chunk stripe names the device it lives on. With one device open
//! there is nothing sensible to do about a stripe naming another —
//! reading it from the device at hand returns whatever is at that
//! offset on THIS disk. It parses. It fails its checksum against a
//! block it was never meant to be, and on a mirrored pool, where both
//! disks hold the same data at different offsets, it may not even do
//! that.
//!
//! So the expected behaviour is a refusal, and this holds the driver to
//! it. Reading one disk of a pool as though it were the whole pool is
//! worse than not opening it: the caller gets data rather than an
//! error.
//!
//! The kernel agrees, which is worth knowing: `mount -o loop` on one
//! member fails too, for the same reason. Mounting a pool needs every
//! device registered first, and that is a different thing from opening
//! a filesystem.
//!
//! The two images and the manifest beside them are built by `chore
//! fixtures`, which makes the pool with the kernel in the harness VM.
//! A missing one fails here, naming that task: a pool test that steps
//! around its own fixture is a pool test that never opened a pool.

use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{fixture, sha256_hex, Image};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Opening one device of a two-device filesystem is refused.
#[test]
fn one_device_of_a_pool_is_refused_rather_than_half_read() {
    let members: Vec<PathBuf> = ["btrfs-pool-a.img", "btrfs-pool-b.img"]
        .iter()
        .map(|n| fixture(n))
        .collect();

    for path in &members {
        let dev = Arc::new(FileDevice::open(path).expect("opening a pool member"));
        // `Filesystem` is not Debug, so the error is taken out by hand
        // rather than through expect_err.
        let msg = match Filesystem::mount(dev) {
            Ok(_) => panic!(
                "{}: a two-device filesystem opened with ONE device was accepted. Every \
                 read of a chunk on the other device now returns whatever lies at that \
                 offset on this one.",
                path.display()
            ),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.contains("spans 2 devices"),
            "the refusal should say what is wrong and how many devices are involved: {msg}"
        );
        eprintln!(
            "{}: refused — {msg}",
            path.file_name().unwrap().to_string_lossy()
        );
    }
}

/// The two halves really are one filesystem, and really are two devices.
///
/// Guards the fixture rather than the driver. If mkfs quietly produced
/// two independent single-device filesystems, the test above would pass
/// for the wrong reason — it would be refusing something that was never
/// a pool.
#[test]
fn the_fixture_really_is_one_filesystem_on_two_devices() {
    let (a, b) = (fixture("btrfs-pool-a.img"), fixture("btrfs-pool-b.img"));

    let read = |p: &Path| -> ([u8; 16], u64, u64) {
        // The superblock alone: these members are 512 MiB each and only
        // 4 KiB of one is wanted. See `Image`.
        let raw = Image::open(p).read_at(0x1_0000, 0x1000);
        let sb = &raw[..];
        let mut fsid = [0u8; 16];
        fsid.copy_from_slice(&sb[0x20..0x30]);
        (
            fsid,
            u64::from_le_bytes(sb[0x88..0x90].try_into().unwrap()),
            // The embedded dev_item's devid.
            u64::from_le_bytes(sb[0xc9..0xd1].try_into().unwrap()),
        )
    };

    let (fsid_a, devs_a, id_a) = read(&a);
    let (fsid_b, devs_b, id_b) = read(&b);

    assert_eq!(fsid_a, fsid_b, "the two images are not the same filesystem");
    assert_eq!(devs_a, 2, "device a does not say it is part of a pair");
    assert_eq!(devs_b, 2, "device b does not say it is part of a pair");
    assert_ne!(id_a, id_b, "both images claim to be the same device");
    eprintln!("one filesystem, devices {id_a} and {id_b}");
}

/// Both devices together: the pool reads, and holds what the kernel put
/// in it.
///
/// This is the point of the whole exercise. `btrfs-pool.manifest` was
/// written by the kernel while the filesystem was mounted, so it says
/// what is really there — including a 4 MiB file, which is large enough
/// to live in a data extent rather than inline in its item and so
/// actually exercises the chunk mapping across two devices.
#[test]
fn a_pool_opened_with_every_device_reads_what_the_kernel_wrote() {
    let (a, b) = (fixture("btrfs-pool-a.img"), fixture("btrfs-pool-b.img"));
    let manifest = fixture("btrfs-pool.manifest");
    let expected = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|error| panic!("reading {}: {error}", manifest.display()));

    let devices: Vec<Arc<dyn fs_core::BlockRead>> = [a, b]
        .iter()
        .map(|p| {
            Arc::new(FileDevice::open(p).expect("opening a pool member"))
                as Arc<dyn fs_core::BlockRead>
        })
        .collect();

    let fs = match Filesystem::mount_pool(devices) {
        Ok(fs) => fs,
        Err(e) => panic!("a pool given all its devices should open: {e}"),
    };

    let mut checked = 0usize;
    for line in expected.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let (path, size, digest) = (parts[0], parts[1], parts[2]);

        if size == "dir" {
            fs.list_path(path)
                .unwrap_or_else(|e| panic!("listing {path}: {e}"));
            checked += 1;
            continue;
        }

        let want: usize = size.parse().expect("a size");
        let got = fs
            .read_path(path)
            .unwrap_or_else(|e| panic!("reading {path}: {e}"));
        assert_eq!(
            got.len(),
            want,
            "{path}: the kernel wrote {want} bytes and this read {}",
            got.len()
        );

        // Content, not just length. On a MIRRORED pool both disks hold
        // the same data at different offsets, so a read that went to
        // the wrong device would come back the right length — and, for
        // RAID1, even the right bytes. The digest is what makes this
        // a check on the data rather than on the arithmetic.
        assert_eq!(
            sha256_hex(&got),
            digest,
            "{path}: {want} bytes read, but not the bytes the kernel wrote"
        );
        checked += 1;
    }

    assert!(checked > 0, "the manifest named nothing to check");
    eprintln!("{checked} entries read back from a two-device pool");
}

/// A pool given devices from two different filesystems is refused.
#[test]
fn devices_from_different_filesystems_are_refused() {
    let (a, other) = (fixture("btrfs-pool-a.img"), fixture("btrfs-default.img"));

    let devices: Vec<Arc<dyn fs_core::BlockRead>> = [a, other]
        .iter()
        .map(|p| Arc::new(FileDevice::open(p).expect("opening")) as Arc<dyn fs_core::BlockRead>)
        .collect();

    match Filesystem::mount_pool(devices) {
        Ok(_) => panic!(
            "two unrelated filesystems were accepted as one pool. Their chunk maps do \
             not describe each other, so every read that crossed would return the wrong \
             disk's bytes."
        ),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("different filesystems"),
                "the refusal should say what is wrong: {msg}"
            );
            eprintln!("mixed devices refused — {msg}");
        }
    }
}
