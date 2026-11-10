//! A preallocated extent is refused as preallocated, not as a hole (#74).
//!
//! `file_extents` dropped a preallocated extent along with the holes, so a
//! write into a `fallocate`d range was refused as "a hole, and filling it
//! would allocate", and `can_write_in_place` said yes to a file that was
//! entirely preallocated.
//!
//! No btrfs-progs tool writes a preallocated extent without a mount, so the
//! fixture makes one: `mkfs.btrfs --rootdir` of one 64 KiB file, the inode
//! marked nodatacow and nodatasum (the only files this driver writes) and
//! its extent item's type changed from regular to prealloc, in every copy
//! of its fs-tree leaf, restamped. `mkfs.btrfs` runs in the harness VM,
//! which is the one place the btrfs-progs tools live, so it is always
//! there and nothing here skips.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs::write::{INODE_NODATACOW, INODE_NODATASUM};
use fs_btrfs_test_support::{le64, oracle, temp_path};
use fs_core::{BlockDevice, BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
const LEN: usize = 64 * 1024;
/// `btrfs_inode_item.flags`.
const INODE_FLAGS: usize = 64;
/// `btrfs_file_extent_item.type`, and its prealloc value.
const EXTENT_TYPE: usize = 20;
const EXTENT_PREALLOC: u8 = 2;
const INODE_ITEM_KEY: u8 = 1;
const EXTENT_DATA_KEY: u8 = 108;

/// One 64 KiB file, made into an image by `mkfs.btrfs --rootdir`.
///
/// The scratch tree lives inside this repository, because the guest that
/// runs `mkfs.btrfs` sees this repository and nothing else of the host:
/// an image under the host's `/tmp` is a path the tool cannot open.
fn image() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("prealloc"));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let body: Vec<u8> = (0..LEN).map(|i| (i % 251) as u8).collect();
    std::fs::write(root.join("file.bin"), body).unwrap();
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-f", "--rootdir"])
        .arg(&root)
        .arg(&img)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    img
}

/// Apply `edit(key_type, item_body)` to every item of `ino` in every copy
/// of every fs-tree leaf, restamping the leaves it touched. Returns how
/// many leaves were rewritten.
fn edit_items(img: &std::path::Path, ino: u64, edit: impl Fn(u8, &mut [u8])) -> usize {
    let mut bytes = std::fs::read(img).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).unwrap();
    let node = sb.nodesize as usize;
    let mut patched = 0;
    for at in (0..bytes.len() - node).step_by(4096) {
        let block = &mut bytes[at..at + node];
        if block[header_offsets::FSID..header_offsets::FSID + 16] != sb.fsid
            || le64(block, header_offsets::OWNER) != objectid::FS_TREE
            || block[header_offsets::LEVEL] != 0
        {
            continue;
        }
        let nritems = u32::from_le_bytes(
            block[header_offsets::NRITEMS..header_offsets::NRITEMS + 4]
                .try_into()
                .unwrap(),
        );
        let mut hit = false;
        for i in 0..nritems as usize {
            let item = HEADER_SIZE + i * ITEM_SIZE;
            if le64(block, item) != ino {
                continue;
            }
            let key_type = block[item + 8];
            let off = HEADER_SIZE
                + u32::from_le_bytes(block[item + 17..item + 21].try_into().unwrap()) as usize;
            let size = u32::from_le_bytes(block[item + 21..item + 25].try_into().unwrap()) as usize;
            edit(key_type, &mut block[off..off + size]);
            hit = true;
        }
        if hit {
            stamp_checksum(block, &sb);
            patched += 1;
        }
    }
    std::fs::write(img, &bytes).unwrap();
    patched
}

#[test]
fn a_preallocated_extent_is_refused_as_preallocated() {
    let img = image();
    let ino = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()) as Arc<dyn BlockRead>)
        .unwrap()
        .lookup_path("/file.bin")
        .unwrap()
        .ino;
    let patched = edit_items(&img, ino, |key_type, body| match key_type {
        INODE_ITEM_KEY => {
            let flags = le64(body, INODE_FLAGS);
            body[INODE_FLAGS..INODE_FLAGS + 8]
                .copy_from_slice(&(flags | INODE_NODATACOW | INODE_NODATASUM).to_le_bytes());
        }
        EXTENT_DATA_KEY => {
            assert_eq!(
                body[EXTENT_TYPE], 1,
                "fixture: the file's extent is regular"
            );
            body[EXTENT_TYPE] = EXTENT_PREALLOC;
        }
        _ => {}
    });
    assert!(patched >= 1, "fixture: the file's leaf was found");

    // Control, read-only: a preallocated extent reads as zeros.
    {
        let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()) as Arc<dyn BlockRead>)
            .unwrap();
        assert!(
            fs.read_path("/file.bin").unwrap().iter().all(|&b| b == 0),
            "fixture: the extent is preallocated"
        );
    }

    let fs =
        Filesystem::mount_rw(Arc::new(FileDevice::open_rw(&img).unwrap()) as Arc<dyn BlockDevice>)
            .unwrap();
    assert!(
        !fs.can_write_in_place(ino).unwrap(),
        "a preallocated file cannot be written in place"
    );
    match fs.write_at(ino, 0, &[0x55; 16]) {
        Err(e) => {
            let why = format!("{e}");
            assert!(
                why.contains("preallocated"),
                "refused, but not as preallocated: {why}"
            );
            assert!(!why.contains("is a hole"), "{why}");
        }
        Ok(_) => panic!("a write into a preallocated extent went through"),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
