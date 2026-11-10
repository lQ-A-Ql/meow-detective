//! A read cannot follow an extent item's window outside the extent the
//! extent tree records (#188).
//!
//! `decode_extent` bounds `offset + num_bytes` by `ram_bytes`, which is a
//! field of the same item, so an item that raises both passes. The write
//! path was fixed against exactly that in #156, by checking the window
//! against the extent tree's own record of the extent's length — the one
//! field that comes from another tree — and the read path was left saying
//! so in a comment.
//!
//! What a read did instead was answer with whatever occupies the addresses
//! past the extent: another file's data, or a tree block. Silently, so a
//! caller could not tell. On an untrusted image, which is what this crate's
//! refusals are written for, that is a disclosure of unrelated on-disk
//! contents through an ordinary `read_file`.
//!
//! The image is `mkfs.btrfs --rootdir` of one 64 KiB file whose extent
//! item's `offset` is moved to 64 KiB and `ram_bytes` raised to 2^40, in
//! every copy of its leaf, restamped — the fixture `extent_window_write.rs`
//! uses for the write half. `mkfs.btrfs` runs in the harness VM, which is
//! the one place the btrfs-progs tools live, so it is always there and
//! nothing here skips.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs_test_support::{le64, oracle, temp_path};
use fs_core::{BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
const LEN: usize = 64 * 1024;
const RAM_BYTES: usize = 8;
const OFFSET: usize = 37;
const EXTENT_DATA_KEY: u8 = 108;

/// One 64 KiB file of a recognisable pattern, on its own volume.
///
/// The scratch tree lives inside this repository, because the guest that
/// runs `mkfs.btrfs` sees this repository and nothing else of the host:
/// an image under the host's `/tmp` is a path the tool cannot open.
fn image(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("extent-window-read-{name}"));
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

fn mount(img: &std::path::Path) -> Filesystem {
    Filesystem::mount(Arc::new(FileDevice::open(img).unwrap()) as Arc<dyn BlockRead>).unwrap()
}

fn ino_of(img: &std::path::Path) -> u64 {
    mount(img).lookup_path("/file.bin").unwrap().ino
}

/// Control: the file as made reads back its own bytes, so the refusal
/// below is the window rather than the fixture.
#[test]
fn the_file_as_made_reads_back() {
    let img = image("control");
    let ino = ino_of(&img);
    let read = mount(&img).read_file(ino).expect("an ordinary read");
    assert_eq!(read.len(), LEN);
    assert!(
        read.iter().enumerate().all(|(i, b)| *b == (i % 253) as u8),
        "the file did not read back what was written to it"
    );
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

#[test]
fn a_window_moved_past_its_extent_is_refused_rather_than_read() {
    let img = image("moved");
    let ino = ino_of(&img);
    let patched = edit_items(&img, ino, |key_type, body| {
        if key_type == EXTENT_DATA_KEY {
            body[RAM_BYTES..RAM_BYTES + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
            body[OFFSET..OFFSET + 8].copy_from_slice(&(LEN as u64).to_le_bytes());
        }
    });
    assert!(patched >= 1, "fixture: the file's leaf was found");

    match mount(&img).read_file(ino) {
        Err(e) => {
            let said = e.to_string();
            assert!(
                said.contains("outside the"),
                "refused, but not by the extent's recorded length: {said}"
            );
        }
        Ok(read) => panic!(
            "read {} bytes through a window outside its extent; the first eight are {:?}",
            read.len(),
            &read[..read.len().min(8)]
        ),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
