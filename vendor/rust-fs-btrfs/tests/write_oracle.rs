//! An in-place write must leave a filesystem `btrfs check` still calls
//! valid, holding exactly the bytes we asked for.
//!
//! Reading our own write back through our own driver would prove only
//! self-consistency: the same misunderstanding of the chunk map would
//! place the write and then find it again. So the bytes are read back by
//! the Linux kernel through its own driver, and the checker then
//! inspects the whole volume — a write that landed correctly could still
//! have run past its extent into something the file's own contents would
//! never reveal.
//!
//! The write is this crate's; the two judgements are not, and neither of
//! them happens here. `btrfs check` and the mount both run in the
//! fs-linux-test-harness VM, which is the one place the tools and a
//! btrfs kernel exist — reached through `fs_btrfs_test_support`, which
//! fails the test when the VM or the fixture is missing rather than
//! letting it pass on no evidence.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, fixture, guest_kernel_read_ok, sha256_hex, temp_path,
};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A `chattr +C` file: written in place, unchecksummed, unshared.
const INPLACE: &str = "/nc/inplace.bin";
/// An ordinary file on the same volume, which must be refused.
const COW: &str = "/cow.bin";

/// A working copy under the scratch directory, removed when it drops —
/// including on a panic. The fixture itself is never written to: it is
/// an input to the whole suite, and half a gigabyte of it, so each test
/// takes its own copy and gives the space back.
struct Scratch(PathBuf);

impl Scratch {
    fn from(source: &Path, name: &str) -> Self {
        let path = PathBuf::from(temp_path!("{name}"));
        std::fs::copy(source, &path).expect("copy the fixture");
        Scratch(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    /// The path as the guest will be given it: the harness mounts this
    /// repository at the same absolute path there.
    fn guest_path(&self) -> &str {
        self.0.to_str().expect("the scratch path is text")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn ino_of(fs: &Filesystem, path: &str) -> u64 {
    fs.lookup_path(path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .ino
}

/// The whole point: overwrite a `nodatacow` file and have Linux agree.
#[test]
fn an_in_place_write_survives_the_kernel_and_the_checker() {
    let source = fixture("btrfs-nodatacow.img");
    let scratch = Scratch::from(&source, "btrfs-write.img");
    let img = scratch.path();

    let (offset, payload) = (8192u64, b"in-place, no copy-on-write\n".repeat(8));
    let expected = {
        let dev = FileDevice::open(img).expect("open read-only");
        let fs = Filesystem::mount(Arc::new(dev)).expect("mount read-only");
        let ino = ino_of(&fs, INPLACE);
        let mut whole = fs.read_file(ino).expect("read the file");
        assert!(
            offset as usize + payload.len() < whole.len(),
            "the payload must land inside the file, not extend it"
        );
        whole[offset as usize..offset as usize + payload.len()].copy_from_slice(&payload);
        sha256_hex(&whole)
    };

    {
        let dev = FileDevice::open_rw(img).expect("open read-write");
        let fs = Filesystem::mount_rw(Arc::new(dev)).expect("mount read-write");
        let ino = ino_of(&fs, INPLACE);
        let n = fs
            .write_at(ino, offset, &payload)
            .expect("the write must be accepted");
        assert_eq!(n, payload.len(), "a short write should not be possible");
    }

    // The checker first, on the image as it lies: a write that ran past
    // its extent is visible to it and to nothing else.
    assert_btrfs_check_clean(img, "after an in-place write");

    // Then the kernel, which is the only reader whose agreement means
    // the bytes are really there. One guest call: mount, hash, unmount.
    let got = guest_kernel_read_ok(
        scratch.guest_path(),
        "in-place write",
        &format!(r#"sha256sum "$MNT{INPLACE}" | cut -d' ' -f1"#),
    );
    assert_eq!(
        got.trim(),
        expected,
        "the kernel reads back different bytes than were written"
    );
}

/// An ordinary copy-on-write file on the same volume must be refused.
///
/// Without this the test above would pass equally well on a driver that
/// wrote in place regardless of the flag — which is precisely the bug
/// worth guarding against, since such a driver would corrupt every
/// normal Btrfs file while looking correct on this fixture.
#[test]
fn a_copy_on_write_file_is_refused() {
    let source = fixture("btrfs-nodatacow.img");
    let scratch = Scratch::from(&source, "btrfs-cow-refused.img");
    let img = scratch.path();

    let before = {
        let dev = FileDevice::open(img).expect("open");
        let fs = Filesystem::mount(Arc::new(dev)).expect("mount");
        sha256_hex(&fs.read_file(ino_of(&fs, COW)).expect("read"))
    };

    let dev = FileDevice::open_rw(img).expect("open read-write");
    let fs = Filesystem::mount_rw(Arc::new(dev)).expect("mount read-write");
    let ino = ino_of(&fs, COW);
    let err = fs
        .write_at(ino, 0, b"this must not land")
        .expect_err("a copy-on-write file must be refused");
    assert!(
        format!("{err}").contains("copy-on-write"),
        "the refusal should name why: {err}"
    );

    let after = sha256_hex(&fs.read_file(ino).expect("read"));
    assert_eq!(before, after, "a refused write still changed the file");
}

/// A read-only mount refuses, and leaves the volume untouched.
#[test]
fn a_read_only_mount_refuses_to_write() {
    let source = fixture("btrfs-nodatacow.img");
    let scratch = Scratch::from(&source, "btrfs-ro.img");
    let img = scratch.path();

    let dev = FileDevice::open(img).expect("open read-only");
    let fs = Filesystem::mount(Arc::new(dev)).expect("mount read-only");
    let ino = ino_of(&fs, INPLACE);
    let err = fs
        .write_at(ino, 0, b"nope")
        .expect_err("a read-only mount must refuse");
    assert!(matches!(err, fs_btrfs::Error::ReadOnly), "got {err}");
}
