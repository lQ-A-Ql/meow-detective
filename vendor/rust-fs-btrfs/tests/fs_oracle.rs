//! The filesystem layer, read against filesystems the Linux kernel made.
//!
//! Everything below the `Filesystem` handle — superblock, chunk map,
//! B-tree — is already cross-validated against the reference tooling.
//! This file checks the layer above: that the driver resolves paths,
//! lists directories and returns file contents that match what the
//! kernel actually wrote.
//!
//! The `deep4k` and `deep16k` fixtures are built by mounting a real
//! Btrfs filesystem and writing 20,000 and 60,000 files into `/many/`,
//! each named `f<N>.txt` and containing the decimal `<N>` and a newline.
//! That gives an exactly known expected value for every file without
//! shipping a manifest, and it is enough files to push the fs tree to
//! level 2, so reads go through real multi-level descent.
//!
//! The fixtures are gitignored and built by `chore fixtures`, inside
//! the fs-linux-test-harness VM — the kernel that wrote `/many/` is the
//! guest's. A fixture that is not there fails the test that asked for
//! it rather than excusing it: a test that printed "skipping" and
//! returned read exactly like one that passed.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{fixture, fixtures_matching, spans_several_devices};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every fixture, with the label a failure names it by.
fn fixtures() -> Vec<(String, PathBuf)> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // A pool member is not a fixture for this file. These tests
        // require every image to MOUNT, and a filesystem spanning two
        // devices opened with one is refused on purpose — reading it
        // would return the wrong data rather than fail. Asserting that
        // every image mounts would turn that correct refusal into a
        // failure.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so not one of them is an \
         image this file is allowed to mount"
    );
    images
        .into_iter()
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
        .collect()
}

/// The populated fixtures, whose `/many/` holds files whose contents
/// are known from their names.
fn deep_fixtures() -> Vec<(String, PathBuf)> {
    fixtures_matching("btrfs-deep")
        .into_iter()
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
        .collect()
}

fn mount(img: &Path, label: &str) -> Filesystem {
    let dev = FileDevice::open(img).unwrap_or_else(|e| panic!("{label}: open: {e}"));
    Filesystem::mount(Arc::new(dev)).unwrap_or_else(|e| panic!("{label}: mount: {e}"))
}

/// Every fixture must mount and expose a root directory.
#[test]
fn every_fixture_mounts_and_has_a_root() {
    for (label, img) in &fixtures() {
        let fs = mount(img, label);
        let root = fs
            .root_inode()
            .unwrap_or_else(|e| panic!("{label}: root inode: {e}"));
        assert!(root.is_dir(), "{label}: the root inode is not a directory");

        // Listing the root must succeed even when it is empty.
        let entries = fs
            .read_dir(root.ino)
            .unwrap_or_else(|e| panic!("{label}: listing the root: {e}"));
        assert!(
            entries.iter().all(|e| e.name != b"." && e.name != b".."),
            "{label}: `.` or `..` leaked into the listing"
        );
        eprintln!("  {label}: root has {} entries", entries.len());
    }
}

/// The populated fixtures let every file's contents be predicted
/// exactly, so this is a content check rather than a smoke test.
#[test]
fn reads_back_the_files_the_kernel_wrote() {
    for (label, img) in &deep_fixtures() {
        let fs = mount(img, label);

        let many = fs
            .lookup_path("/many")
            .unwrap_or_else(|e| panic!("{label}: /many should exist: {e}"));
        assert!(many.is_dir(), "{label}: /many is not a directory");

        let entries = fs
            .read_dir(many.ino)
            .unwrap_or_else(|e| panic!("{label}: listing /many: {e}"));
        let expected = if label.contains("4k") { 20_000 } else { 60_000 };
        assert_eq!(
            entries.len(),
            expected,
            "{label}: /many should hold {expected} entries"
        );

        // Sample across the whole range rather than the first few, so
        // the reads exercise different subtrees.
        let step = expected / 40;
        let mut checked = 0usize;
        for n in (1..=expected).step_by(step) {
            let path = format!("/many/f{n}.txt");
            let data = fs
                .read_path(&path)
                .unwrap_or_else(|e| panic!("{label}: reading {path}: {e}"));
            let want = format!("{n}\n");
            assert_eq!(
                String::from_utf8_lossy(&data),
                want,
                "{label}: {path} contents differ from what the kernel wrote"
            );
            checked += 1;
        }
        assert!(checked >= 20, "{label}: only {checked} files sampled");
        eprintln!(
            "  {label}: {} entries, {checked} files byte-exact",
            entries.len()
        );
    }
}

/// Names in the listing must resolve, and resolve to the inode the
/// listing named. A listing that reports entries a lookup cannot find is
/// worse than an empty one.
#[test]
fn every_listed_name_resolves_to_the_inode_it_named() {
    for (label, img) in &fixtures() {
        let fs = mount(img, label);
        let root = fs.root_inode().expect("root");
        for e in fs.read_dir(root.ino).expect("listing") {
            let name = String::from_utf8_lossy(&e.name).into_owned();

            // A subvolume is listed in its parent directory but names a
            // tree rather than an inode, so there is nothing in this
            // tree to resolve it to. The refusal has to be the specific
            // one — a generic NotFound for a name that is plainly there
            // would pass this check while telling a reader the opposite
            // of the truth.
            if !e.is_inode() {
                let err = fs
                    .lookup(root.ino, &e.name)
                    .expect_err("a subvolume entry cannot resolve to an inode here");
                assert!(
                    err.to_string().contains("subvolume"),
                    "{label}: `{name}` names a subvolume, and the refusal should say so: {err}"
                );
                continue;
            }

            let found = fs
                .lookup(root.ino, &e.name)
                .unwrap_or_else(|err| panic!("{label}: `{name}` was listed but not found: {err}"));
            assert_eq!(
                found.ino, e.ino,
                "{label}: `{name}` resolved to a different inode than the listing gave"
            );
        }
    }
}

/// Refusals, which are half of what a filesystem driver is for. A driver
/// that returns something for a case it does not understand hands a user
/// silently wrong data with no way to detect it.
#[test]
fn refuses_what_it_cannot_answer() {
    // `deep_fixtures` has already refused an empty list.
    let deep = deep_fixtures();
    let (label, img) = &deep[0];
    let fs = mount(img, label);

    assert!(
        matches!(
            fs.lookup_path("/definitely-absent"),
            Err(fs_btrfs::Error::NotFound)
        ),
        "a missing path must be NotFound"
    );
    assert!(
        matches!(
            fs.lookup_path("/many/f1.txt/child"),
            Err(fs_btrfs::Error::NotADirectory)
        ),
        "descending through a file must be NotADirectory"
    );
    assert!(
        matches!(
            fs.lookup_path("/many/../many"),
            Err(fs_btrfs::Error::UnsupportedFeature(_))
        ),
        "`..` must be declined rather than silently resolved"
    );

    // Reading a directory's bytes as file contents is not meaningful.
    let many = fs.lookup_path("/many").expect("/many");
    assert!(
        matches!(fs.read_file(many.ino), Err(fs_btrfs::Error::NotAFile)),
        "reading a directory as a file must be refused"
    );

    // A read starting past end of file is empty, not an error.
    let f = fs.lookup_path("/many/f1.txt").expect("f1");
    let mut buf = [0u8; 16];
    assert_eq!(fs.read_at(f.ino, f.size + 100, &mut buf).expect("read"), 0);
}

/// Redundant separators and `.` components are ordinary, and the root is
/// reachable by each of its spellings.
#[test]
fn path_spellings_are_tolerated() {
    let deep = deep_fixtures();
    let (label, img) = &deep[0];
    let fs = mount(img, label);

    let direct = fs.lookup_path("/many/f1.txt").expect("direct");
    for messy in ["//many//f1.txt", "/./many/./f1.txt", "many/f1.txt"] {
        let got = fs
            .lookup_path(messy)
            .unwrap_or_else(|e| panic!("{label}: `{messy}` should resolve: {e}"));
        assert_eq!(got.ino, direct.ino, "`{messy}` resolved elsewhere");
    }

    let root = fs.root_inode().expect("root").ino;
    for spelling in ["/", "", ".", "/./"] {
        assert_eq!(
            fs.lookup_path(spelling).expect("root resolves").ino,
            root,
            "`{spelling}` did not resolve to the root"
        );
    }
}

// ---------------------------------------------------------------------
// The `rich` fixture: written through a compressing mount, and holding
// a symlink, a sparse file and an inline file. It exercises the paths a
// plain mkfs image never reaches.
// ---------------------------------------------------------------------

fn rich() -> Filesystem {
    mount(&fixture("btrfs-rich.img"), "btrfs-rich")
}

/// A compressed extent must come back as the file the kernel wrote.
///
/// This replaces the refusal that stood here before compression was
/// decoded. The refusal existed because returning compressed bytes looks
/// to a caller exactly like a successful read of a corrupt file, with no
/// signal to tell them apart — so the bar for removing it is that the
/// bytes now match what Linux says the file holds, not merely that a
/// read succeeds. That comparison lives in `compression_oracle.rs`,
/// which drives it from kernel-generated manifests for all three
/// algorithms. This keeps a direct check on the original fixture.
#[test]
fn reads_a_compressed_file_written_by_the_kernel() {
    let fs = rich();
    let f = fs
        .lookup_path("/compressed.txt")
        .expect("compressed.txt should exist");
    let data = fs.read_file(f.ino).expect("compressed.txt must decode");
    assert_eq!(
        data.len() as u64,
        f.size,
        "decoded length disagrees with the inode's size"
    );
    // The fixture is one sentence repeated, so its content is known
    // without needing the manifest to say so.
    let text = String::from_utf8(data).expect("the fixture is ASCII");
    assert!(
        text.starts_with("the quick brown fox jumps over the lazy dog "),
        "decoded to something other than the fixture's text: {:?}",
        &text[..text.len().min(60)]
    );
    assert_eq!(
        text.matches("the quick brown fox").count(),
        20000,
        "decoded the wrong number of repetitions"
    );
}

/// An incompressible file written through the same compressing mount
/// stays a plain extent, so it must still read correctly. Without this,
/// the test above would pass equally well on a driver that refused
/// everything.
#[test]
fn still_reads_uncompressed_files_from_a_compressing_mount() {
    let fs = rich();
    let f = fs.lookup_path("/plain.bin").expect("plain.bin");
    let data = fs.read_file(f.ino).expect("plain.bin must still read");
    assert_eq!(data.len() as u64, f.size, "short read of an ordinary file");
    assert!(
        data.iter().any(|&b| b != 0),
        "random data read back as all zeros"
    );
}

/// A small file lives inline in its item rather than in an extent.
#[test]
fn reads_an_inline_file() {
    let fs = rich();
    let data = fs.read_path("/inline.txt").expect("inline.txt");
    assert_eq!(String::from_utf8_lossy(&data), "small inline\n");
}

/// A sparse file is almost entirely holes, which must read as zeros
/// rather than as whatever previously occupied those blocks.
#[test]
fn sparse_regions_read_as_zeros() {
    let fs = rich();
    let f = fs.lookup_path("/sparse.bin").expect("sparse.bin");
    let data = fs.read_file(f.ino).expect("sparse read");
    assert_eq!(data.len(), 8 * 1024 * 1024);
    assert!(
        data.iter().all(|&b| b == 0),
        "a hole did not read back as zeros"
    );
}

#[test]
fn resolves_a_symlink_target() {
    let fs = rich();
    let l = fs.lookup_path("/link-short").expect("link-short");
    assert!(l.is_symlink(), "link-short should be a symlink");
    let target = fs.read_link(l.ino).expect("readlink");
    assert_eq!(String::from_utf8_lossy(&target), "inline.txt");

    // readlink on something that is not a link is refused.
    let f = fs.lookup_path("/inline.txt").expect("inline.txt");
    assert!(matches!(
        fs.read_link(f.ino),
        Err(fs_btrfs::Error::NotAFile)
    ));
}

/// Reads at an offset must agree with reading the whole file and
/// slicing, which catches an offset mishandled in the extent walk.
#[test]
fn partial_reads_agree_with_whole_file_reads() {
    let fs = rich();
    let f = fs.lookup_path("/plain.bin").expect("plain.bin");
    let whole = fs.read_file(f.ino).expect("whole");
    for &(off, len) in &[
        (0u64, 100usize),
        (4095, 2),
        (4096, 4096),
        (1_000_003, 9973),
        (whole.len() as u64 - 10, 10),
    ] {
        let mut buf = vec![0u8; len];
        let n = fs.read_at(f.ino, off, &mut buf).expect("read_at");
        assert_eq!(
            &buf[..n],
            &whole[off as usize..off as usize + n],
            "read_at({off}, {len}) disagrees with the whole-file read"
        );
    }
}

/// Nested directories resolve, and a directory's own listing round-trips
/// through path resolution.
#[test]
fn walks_nested_directories() {
    let fs = rich();
    let data = fs.read_path("/sub/nested/file.txt").expect("nested file");
    assert_eq!(String::from_utf8_lossy(&data), "nested\n");

    let listing = fs.list_path("/sub").expect("list /sub");
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, b"nested");
}

/// An inode number that names nothing must be NotFound rather than a
/// panic or an empty success.
#[test]
fn an_unknown_inode_is_not_found() {
    let fs = rich();
    assert!(matches!(
        fs.read_inode(999_999_999),
        Err(fs_btrfs::Error::NotFound)
    ));
}
