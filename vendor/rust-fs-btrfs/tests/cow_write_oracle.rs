//! A write to an ordinary copy-on-write file lands in a new extent, and
//! the kernel and `btrfs check` both agree with the result (#61).
//!
//! # What this covers
//!
//! The first slice of copy-on-write file writing: a file that is
//! copy-on-write but not checksummed, which is what the kernel creates
//! on a volume mounted `-o nodatasum`. Each extent the write touches is
//! copied whole into a newly allocated one with the write applied, the
//! file's `EXTENT_DATA` item is pointed at the copy, the extent tree
//! records the new extent and forgets the old one, the free-space tree
//! and the block groups' `used` follow, and the transaction is committed.
//! A checksummed file is still refused, as is an extent a snapshot shares.
//!
//! # The oracles
//!
//! The volume is made by `mkfs.btrfs` and filled by the kernel, in the
//! harness VM. After this crate writes it, `btrfs check` inspects the
//! whole volume — a leaked or doubly-referenced extent, a free-space tree
//! that disagrees with the extent tree, or a block group whose `used` is
//! wrong are each reported there and nowhere else — and then the kernel
//! mounts it and reads every byte back. `filefrag` asks the kernel where
//! each file's extent now lies, which is how the test knows the write was
//! copy-on-write and not in place.

use fs_btrfs::fs::Filesystem;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, guest_kernel_read_ok, guest_kernel_write_ok, oracle, sha256_hex,
    temp_path,
};
use fs_core::{BlockDevice, FileDevice};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One extent of 64 KiB, unchecksummed.
const ONE: &str = "one.bin";
/// Two extents — 64 KiB, then 40,000 bytes appended in a later
/// transaction — so the file ends partway through its last sector.
const TWO: &str = "two.bin";
/// Written before the remount, so an ordinary checksummed file.
const SUMMED: &str = "summed.bin";

/// A scratch volume, removed when it drops — including on a panic.
struct Scratch(PathBuf);

impl Scratch {
    /// A fresh 256 MiB volume from `mkfs.btrfs`, filled by `script` run
    /// against a read-write kernel mount.
    fn new(name: &str, script: &str) -> Self {
        let dir = PathBuf::from(temp_path!("{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("fs.img");
        std::fs::File::create(&image)
            .and_then(|f| f.set_len(256 << 20))
            .unwrap();
        let made = oracle("mkfs.btrfs")
            .args(["-q", "-f", "-s", "4096", "-n", "16384"])
            .arg(&image)
            .output();
        assert!(
            made.status.success(),
            "mkfs.btrfs: {}",
            String::from_utf8_lossy(&made.stderr)
        );
        guest_kernel_write_ok(&image.to_string_lossy(), name, script);
        Scratch(dir)
    }

    fn image(&self) -> PathBuf {
        self.0.join("fs.img")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mount_rw(image: &Path) -> Filesystem {
    let dev = Arc::new(FileDevice::open_rw(image).expect("open read-write"));
    Filesystem::mount_rw(dev as Arc<dyn BlockDevice>).expect("mount read-write")
}

fn ino_of(fs: &Filesystem, name: &str) -> u64 {
    fs.lookup_path(&format!("/{name}"))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .ino
}

/// What the kernel says about each named file: its SHA-256 and the
/// physical address of its first extent, from `filefrag`.
fn kernel_view(image: &Path, what: &str, names: &[&str]) -> BTreeMap<String, (String, String)> {
    let mut script = String::new();
    for name in names {
        script.push_str(&format!(
            "printf '%s %s %s\\n' {name} \"$(sha256sum \"$MNT/{name}\" | cut -d' ' -f1)\" \
             \"$(filefrag -v \"$MNT/{name}\" | awk '$1 == \"0:\" {{print $4; exit}}')\"\n"
        ));
    }
    guest_kernel_read_ok(&image.to_string_lossy(), what, &script)
        .lines()
        .map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next().unwrap_or_default().to_string();
            let sha = words.next().unwrap_or_default().to_string();
            let at = words.next().unwrap_or_default().to_string();
            (name, (sha, at))
        })
        .collect()
}

/// The fill every test here starts from.
const FILL: &str = "head -c 65536 /dev/urandom > \"$MNT/summed.bin\"\n\
     sync\n\
     mount -o remount,nodatasum \"$MNT\"\n\
     head -c 65536 /dev/urandom > \"$MNT/one.bin\"\n\
     sync\n\
     head -c 65536 /dev/urandom > \"$MNT/two.bin\"\n\
     sync\n\
     head -c 40000 /dev/urandom >> \"$MNT/two.bin\"\n\
     sync";

/// Writes inside one extent, across the boundary between two, and up to
/// the end of a file that stops partway through a sector — several
/// transactions on one mount, so each plans from what the last committed.
#[test]
fn a_copy_on_write_file_is_written_into_new_extents_the_checker_and_kernel_accept() {
    let scratch = Scratch::new("cow-write", FILL);
    let image = scratch.image();
    let before = kernel_view(&image, "before the write", &[ONE, TWO]);

    // (file, offset, bytes)
    let writes: Vec<(&str, u64, Vec<u8>)> = vec![
        (ONE, 5000, b"copy-on-write, inside one extent ".repeat(3)),
        (TWO, 61440, vec![0xa5; 8192]),
        (TWO, 104_536, vec![0x5a; 1000]),
    ];

    let mut fs = mount_rw(&image);
    let mut expected: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for name in [ONE, TWO] {
        let ino = ino_of(&fs, name);
        let inode = fs.read_inode(ino).unwrap();
        assert_eq!(
            inode.flags & fs_btrfs::write::INODE_NODATACOW,
            0,
            "{name} must be copy-on-write for this to test anything"
        );
        assert_ne!(
            inode.flags & fs_btrfs::write::INODE_NODATASUM,
            0,
            "{name} must be nodatasum: the remount did not take"
        );
        expected.insert(name, fs.read_file(ino).unwrap());
    }
    assert_eq!(expected[TWO].len(), 105_536, "{TWO} is not the size made");

    let generation = fs.superblock().generation;
    for (name, offset, bytes) in &writes {
        let ino = ino_of(&fs, name);
        let n = fs
            .write(ino, *offset, bytes)
            .unwrap_or_else(|e| panic!("writing {} bytes at {offset} of {name}: {e}", bytes.len()));
        assert_eq!(n, bytes.len(), "a short write");
        let file = expected.get_mut(name).unwrap();
        file[*offset as usize..*offset as usize + bytes.len()].copy_from_slice(bytes);
        // Our own reading of what we committed, before the oracles'.
        assert_eq!(
            &fs.read_file(ino).unwrap(),
            file,
            "{name} reads back differently through this driver after the write"
        );
    }
    assert_eq!(
        fs.superblock().generation,
        generation + writes.len() as u64,
        "each write is one committed transaction"
    );
    drop(fs);

    assert_btrfs_check_clean(&image, "after copy-on-write writes");

    let after = kernel_view(&image, "after the write", &[ONE, TWO]);
    for name in [ONE, TWO] {
        assert_eq!(
            after[name].0,
            sha256_hex(&expected[name]),
            "the kernel reads back different bytes from {name} than were written"
        );
        assert_ne!(
            after[name].1, before[name].1,
            "{name}'s first extent is where it was, so the write was not copy-on-write"
        );
    }
}

/// A checksummed file is refused by name, and left exactly as it was:
/// writing it means writing the checksum tree, which this slice does not.
#[test]
fn a_checksummed_copy_on_write_file_is_refused_and_untouched() {
    let scratch = Scratch::new("cow-write-summed", FILL);
    let image = scratch.image();
    let before = std::fs::read(&image).unwrap();

    let mut fs = mount_rw(&image);
    let ino = ino_of(&fs, SUMMED);
    let err = fs
        .write(ino, 0, b"this must not land")
        .expect_err("a checksummed file must be refused");
    assert!(
        err.to_string().contains("checksum"),
        "the refusal should name the checksums: {err}"
    );
    drop(fs);
    assert!(
        std::fs::read(&image).unwrap() == before,
        "a refused write changed the image"
    );
}

/// An extent a snapshot still reads is refused, and the image is left
/// byte for byte as it was: releasing it would free what the snapshot
/// reads.
#[test]
fn a_copy_on_write_extent_a_snapshot_shares_is_refused_and_untouched() {
    let scratch = Scratch::new(
        "cow-write-snapshot",
        "mount -o remount,nodatasum \"$MNT\"\n\
         head -c 65536 /dev/urandom > \"$MNT/one.bin\"\n\
         sync\n\
         btrfs subvolume snapshot -r \"$MNT\" \"$MNT/snap\" >/dev/null\n\
         sync",
    );
    let image = scratch.image();
    let before = std::fs::read(&image).unwrap();

    let mut fs = mount_rw(&image);
    let ino = ino_of(&fs, ONE);
    let err = fs
        .write(ino, 4096, b"this must not land")
        .expect_err("an extent a snapshot shares must be refused");
    assert!(
        err.to_string().contains("snapshot"),
        "the refusal should name the snapshot: {err}"
    );
    drop(fs);
    assert!(
        std::fs::read(&image).unwrap() == before,
        "a refused write changed the image"
    );
}
