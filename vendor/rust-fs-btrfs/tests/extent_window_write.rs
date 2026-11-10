//! A write cannot follow an extent item's window outside the extent the
//! extent tree records (#89).
//!
//! The window check compared `offset + num_bytes` with `ram_bytes`, a field
//! of the same item, so an item raising both moved the write past the end of
//! its extent. The reference check still found the extent by its start and
//! one owner, and the bytes landed wherever the window pointed. Here that is
//! the next 64 KiB of the data chunk, and the whole image is compared before
//! and after.
//!
//! The image is `mkfs.btrfs --rootdir` of one 64 KiB file, marked nodatacow
//! and nodatasum (the only files this driver writes), with its extent item's
//! `offset` moved to 64 KiB and `ram_bytes` raised to 2^40, in every copy of
//! its leaf, restamped. `mkfs.btrfs` runs in the harness VM, which is the one
//! place the btrfs-progs tools live, so it is always there and nothing here
//! skips.

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
const INODE_FLAGS: usize = 64;
const RAM_BYTES: usize = 8;
const OFFSET: usize = 37;
const INODE_ITEM_KEY: u8 = 1;
const EXTENT_DATA_KEY: u8 = 108;

/// One 64 KiB file, made into an image by `mkfs.btrfs --rootdir`.
///
/// The scratch tree lives inside this repository, because the guest that
/// runs `mkfs.btrfs` sees this repository and nothing else of the host:
/// an image under the host's `/tmp` is a path the tool cannot open.
fn image(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("extent-window-{name}"));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let body: Vec<u8> = (0..LEN).map(|i| (i % 253) as u8).collect();
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

/// Apply `edit(key_type, body)` to `ino`'s items in every fs-tree leaf copy.
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

fn mark_nodatacow(body: &mut [u8]) {
    let flags = le64(body, INODE_FLAGS);
    body[INODE_FLAGS..INODE_FLAGS + 8]
        .copy_from_slice(&(flags | INODE_NODATACOW | INODE_NODATASUM).to_le_bytes());
}

fn ino_of(img: &std::path::Path) -> u64 {
    Filesystem::mount(Arc::new(FileDevice::open(img).unwrap()) as Arc<dyn BlockRead>)
        .unwrap()
        .lookup_path("/file.bin")
        .unwrap()
        .ino
}

fn mount_rw(img: &std::path::Path) -> Filesystem {
    Filesystem::mount_rw(Arc::new(FileDevice::open_rw(img).unwrap()) as Arc<dyn BlockDevice>)
        .unwrap()
}

/// Control: the same file, marked writable and left in place, takes the
/// write -- so the refusal below is the window, not the fixture.
#[test]
fn the_file_as_made_is_written_in_place() {
    let img = image("control");
    let ino = ino_of(&img);
    let patched = edit_items(&img, ino, |key_type, body| {
        if key_type == INODE_ITEM_KEY {
            mark_nodatacow(body);
        }
    });
    assert!(patched >= 1);
    let fs = mount_rw(&img);
    assert_eq!(
        fs.write_at(ino, 0, &[0x55; 16]).expect("an ordinary write"),
        16
    );
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

#[test]
fn a_window_moved_past_its_extent_is_refused_before_anything_is_written() {
    let img = image("moved");
    let ino = ino_of(&img);
    let patched = edit_items(&img, ino, |key_type, body| match key_type {
        INODE_ITEM_KEY => mark_nodatacow(body),
        EXTENT_DATA_KEY => {
            body[RAM_BYTES..RAM_BYTES + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
            body[OFFSET..OFFSET + 8].copy_from_slice(&(LEN as u64).to_le_bytes());
        }
        _ => {}
    });
    assert!(patched >= 1, "fixture: the file's leaf was found");

    let fs = mount_rw(&img);
    let before = std::fs::read(&img).unwrap();
    // The question a caller asks first gets the same answer the write
    // gives (Greptile on #156).
    assert!(
        !fs.can_write_in_place(ino).unwrap(),
        "a file whose every write is refused was reported writable"
    );
    match fs.write_at(ino, 0, &[0x55; 16]) {
        Err(e) => assert!(
            format!("{e}").contains("outside the"),
            "refused, but not by the extent's recorded length: {e}"
        ),
        Ok(n) => panic!("wrote {n} bytes through a window outside its extent"),
    }
    drop(fs);
    assert!(
        std::fs::read(&img).unwrap() == before,
        "the image changed although the write was refused"
    );
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

/// Each of the five extent fields must be whole sectors, as the kernel's
/// tree checker requires; a read through an item that is not refuses.
#[test]
fn an_extent_field_off_a_sector_boundary_is_refused_on_read() {
    let img = image("aligned");
    let ino = ino_of(&img);
    let patched = edit_items(&img, ino, |key_type, body| {
        if key_type == EXTENT_DATA_KEY {
            body[OFFSET..OFFSET + 8].copy_from_slice(&1u64.to_le_bytes());
        }
    });
    assert!(patched >= 1);
    let fs =
        Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()) as Arc<dyn BlockRead>).unwrap();
    match fs.read_path("/file.bin") {
        Err(e) => assert!(format!("{e}").contains("not a multiple"), "{e}"),
        Ok(_) => panic!("an extent offset of 1 byte was read through"),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
