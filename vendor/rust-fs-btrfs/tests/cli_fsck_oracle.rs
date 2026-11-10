//! `fsck.btrfs` agrees with `btrfs check --readonly` about what is wrong
//! (#260).
//!
//! Two halves, each against the reference checker in the harness guest:
//!
//! - **Clean volumes stay clean.** Every fixture below was made by the
//!   standard formatter and filled by the kernel. `btrfs check` must call
//!   each clean, and so must `fsck.btrfs` (exit 0).
//! - **Damage is found.** A copy of a populated fixture is damaged one way
//!   at a time: an item's bytes are changed in place and the block's
//!   checksum recomputed and written to every mirror, so each case is the
//!   damage it names and not a checksum failure. `btrfs check` must find
//!   each one (a nonzero status), or the case is not damage and the test
//!   says so; then `fsck.btrfs` must find it too (exit 4).

mod cli_support;

use cli_support::*;
use fs_btrfs::chunk::DiskKey;
use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{fixture, oracle};
use fs_core::{BlockRead, FileDevice};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fixtures the reference checker calls clean.
const CLEAN: &[&str] = &[
    "btrfs-default.img",
    "btrfs-dup.img",
    "btrfs-single.img",
    "btrfs-mixed.img",
    "btrfs-rich.img",
    "btrfs-subvol.img",
    "btrfs-xattr.img",
    "btrfs-nodatacow.img",
    "btrfs-comp-zstd.img",
    "btrfs-csum-sha256.img",
    "btrfs-node4k.img",
    "btrfs-deep16k.img",
];

/// The populated fixture each damage is made in.
const DAMAGED_FROM: &str = "btrfs-rich.img";

/// The size of a tree block's header: item offsets count from its end.
const HEADER: usize = 101;

fn fsck(image: &Path) -> (Option<i32>, String) {
    let out = tool("fsck.btrfs")
        .arg("--text")
        .arg(image)
        .output()
        .expect("spawn fsck.btrfs");
    (
        out.status.code(),
        format!("{}{}", stdout(&out), stderr(&out)),
    )
}

/// `btrfs check --readonly`: clean, and what it said.
fn reference(image: &Path) -> (bool, String) {
    let out = oracle("btrfs")
        .args(["check", "--readonly"])
        .arg(image)
        .output();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn copy(name: &str, tag: &str) -> PathBuf {
    let path = scratch_dir(&format!("fsck-{tag}")).join(name);
    std::fs::copy(fixture(name), &path).unwrap_or_else(|e| panic!("copy {name}: {e}"));
    path
}

#[test]
fn every_volume_the_reference_calls_clean_is_clean() {
    for name in CLEAN {
        let image = copy(name, &format!("clean-{name}"));
        let (clean, said) = reference(&image);
        assert!(
            clean,
            "{name}: btrfs check does not call the fixture clean:\n{said}"
        );
        let (code, ours) = fsck(&image);
        assert_eq!(
            code,
            Some(0),
            "{name}: fsck.btrfs finds fault with a clean volume:\n{ours}"
        );
    }
}

/// The root of tree `tree` in `fs`.
fn root_of(fs: &Filesystem, tree: u64) -> u64 {
    match tree {
        1 => fs.superblock().root,
        3 => fs.superblock().chunk_root,
        _ => fs
            .root_tree_items()
            .unwrap()
            .into_iter()
            .find(|(objectid, key_type, _, _)| *objectid == tree && *key_type == 132)
            .map(|(_, _, _, data)| u64::from_le_bytes(data[176..184].try_into().unwrap()))
            .unwrap_or_else(|| panic!("no root item for tree {tree}")),
    }
}

/// Change the first item of `tree` that `pick` chooses, with `edit`, and
/// write its block back with a fresh checksum to every mirror.
fn edit_item(
    image: &Path,
    tree: u64,
    pick: impl Fn(&DiskKey, &[u8]) -> bool,
    edit: impl Fn(&mut [u8]),
) {
    let fs =
        Filesystem::mount(Arc::new(FileDevice::open(image.to_str().unwrap()).unwrap())).unwrap();
    let mut stack = vec![root_of(&fs, tree)];
    while let Some(logical) = stack.pop() {
        let block = fs.read_tree_block(logical).unwrap();
        match &block.body {
            fs_btrfs::btree::Body::Node(ptrs) => stack.extend(ptrs.iter().map(|p| p.blockptr)),
            fs_btrfs::btree::Body::Leaf(items) => {
                for item in items {
                    let data = block.item_data(item).unwrap();
                    if !pick(&item.key, data) {
                        continue;
                    }
                    let mut bytes = block.bytes().to_vec();
                    let at = HEADER + item.offset as usize;
                    edit(&mut bytes[at..at + item.size as usize]);
                    fs_btrfs::tree_write::stamp_checksum(&mut bytes, fs.superblock());
                    let file = std::fs::OpenOptions::new().write(true).open(image).unwrap();
                    for mirror in 0..fs.chunk_map().mirrors_at(logical).unwrap() {
                        let m = fs.chunk_map().map_mirror(logical, mirror).unwrap();
                        file.write_all_at(&bytes, m.physical).unwrap();
                    }
                    return;
                }
            }
        }
    }
    panic!("no item in tree {tree} matched");
}

fn add_u64(data: &mut [u8], at: usize, delta: u64) {
    let v = u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
    data[at..at + 8].copy_from_slice(&v.wrapping_add(delta).to_le_bytes());
}
fn add_u32(data: &mut [u8], at: usize, delta: u32) {
    let v = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    data[at..at + 4].copy_from_slice(&v.wrapping_add(delta).to_le_bytes());
}

/// A regular file's inode in the default subvolume.
fn is_file_inode(k: &DiskKey, data: &[u8]) -> bool {
    k.key_type == 1
        && k.objectid > 256
        && u32::from_le_bytes(data[52..56].try_into().unwrap()) & 0o170000 == 0o100000
}

/// One way of damaging an image in place.
type Damage = Box<dyn Fn(&Path)>;

#[test]
fn every_damage_the_reference_finds_is_found() {
    let cases: Vec<(&str, Damage)> = vec![
        (
            "link-count",
            Box::new(|p| edit_item(p, 5, is_file_inode, |d| add_u32(d, 40, 1))),
        ),
        (
            "directory-size",
            Box::new(|p| {
                edit_item(
                    p,
                    5,
                    |k, _| k.key_type == 1 && k.objectid == 256,
                    |d| add_u64(d, 16, 2),
                )
            }),
        ),
        (
            "block-group-used",
            Box::new(|p| edit_item(p, 2, |k, _| k.key_type == 192, |d| add_u64(d, 0, 4096))),
        ),
        (
            "device-bytes-used",
            Box::new(|p| edit_item(p, 3, |k, _| k.key_type == 216, |d| add_u64(d, 16, 1 << 20))),
        ),
        (
            "device-extent-length",
            Box::new(|p| edit_item(p, 4, |k, _| k.key_type == 204, |d| add_u64(d, 24, 1 << 20))),
        ),
        (
            "entry-to-no-inode",
            Box::new(|p| {
                edit_item(
                    p,
                    5,
                    |k, _| k.key_type == 96 && k.objectid == 256,
                    |d| add_u64(d, 0, 1000),
                )
            }),
        ),
        (
            "file-extent-length",
            Box::new(|p| {
                edit_item(
                    p,
                    5,
                    |k, d| k.key_type == 108 && d.len() >= 53 && d[20] == 1 && d[21..29] != [0; 8],
                    |d| add_u64(d, 29, 4096),
                )
            }),
        ),
        (
            "superblock-bytes-used",
            Box::new(|p| {
                let fs =
                    Filesystem::mount(Arc::new(FileDevice::open(p.to_str().unwrap()).unwrap()))
                        .unwrap();
                let csum = fs.superblock().csum_type;
                let size = FileDevice::open(p.to_str().unwrap()).unwrap().size_bytes();
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(p)
                    .unwrap();
                for at in [64u64 << 10, 64 << 20, 256 << 30] {
                    if at + 4096 > size {
                        continue;
                    }
                    let mut raw = vec![0u8; 4096];
                    file.read_exact_at(&mut raw, at).unwrap();
                    add_u64(&mut raw, 0x78, 4096);
                    fs_btrfs::super_write::stamp_checksum(&mut raw, csum);
                    file.write_all_at(&raw, at).unwrap();
                }
            }),
        ),
    ];

    for (name, damage) in cases {
        let image = copy(DAMAGED_FROM, name);
        damage(&image);
        let (clean, said) = reference(&image);
        assert!(
            !clean,
            "{name}: btrfs check finds nothing wrong, so this is not damage:\n{said}"
        );
        let (code, ours) = fsck(&image);
        assert_eq!(
            code,
            Some(4),
            "{name}: btrfs check finds the damage and fsck.btrfs does not:\n{ours}\n\
             --- btrfs check said:\n{said}"
        );
    }
}

#[test]
fn a_device_that_is_not_btrfs_is_an_operational_error() {
    let image = scratch_dir("fsck-zero").join("zero.img");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(1 << 20)
        .unwrap();
    let (code, said) = fsck(&image);
    assert_eq!(
        code,
        Some(8),
        "a zeroed device is not Btrfs, and nothing was checked:\n{said}"
    );
}

#[test]
fn a_repair_is_refused_not_pretended() {
    let image = copy("btrfs-default.img", "y");
    let out = tool("fsck.btrfs").arg("-y").arg(&image).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(16),
        "fsck.btrfs -y: {}",
        stderr(&out)
    );
}
