//! The chunk-tree walk takes only chunk items, and a chunk item that is
//! malformed fails the mount by name (#85).
//!
//! The walk handed every item of the chunk tree to `Chunk::parse` and kept
//! whatever parsed, dropping what did not, and never checked geometry. So a
//! malformed `CHUNK_ITEM` became a hole in the address map, surfacing later
//! as an unmapped read with nothing pointing back at the chunk. The image
//! is a fresh `mkfs.btrfs` on a plain file with one chunk item damaged in
//! the chunk tree leaf and the leaf's checksum restamped. `mkfs.btrfs` runs
//! in the harness VM, the one place this suite has btrfs-progs at all, so
//! there is no absent tool to step around.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::{key_type, ChunkMap};
use fs_btrfs::error::Error;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
/// `BTRFS_BLOCK_GROUP_SYSTEM`.
const SYSTEM: u64 = 1 << 1;

/// A fresh image with `damage` applied to the body of the first non-system
/// chunk item in the chunk tree's root leaf.
///
/// The image lives under the suite's scratch directory inside this
/// repository, because that is the tree `mkfs.btrfs` can see from the
/// harness VM it runs in.
fn image_with_damaged_chunk(name: &str, damage: fn(&mut [u8])) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("chunk-items-{name}"));
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

    let mut bytes = std::fs::read(&path).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).expect("superblock");
    let map = ChunkMap::bootstrap(&sb).expect("bootstrap map");
    let at = map
        .map(sb.chunk_root)
        .expect("chunk root is mapped")
        .physical as usize;
    let node = sb.nodesize as usize;
    let leaf = &mut bytes[at..at + node];
    assert_eq!(
        leaf[header_offsets::LEVEL],
        0,
        "fixture: the chunk root is a leaf"
    );
    let nritems = u32::from_le_bytes(
        leaf[header_offsets::NRITEMS..header_offsets::NRITEMS + 4]
            .try_into()
            .unwrap(),
    );
    let mut damaged = false;
    for i in 0..nritems as usize {
        let item = HEADER_SIZE + i * ITEM_SIZE;
        if leaf[item + 8] != key_type::CHUNK_ITEM {
            continue;
        }
        let off = HEADER_SIZE
            + u32::from_le_bytes(leaf[item + 17..item + 21].try_into().unwrap()) as usize;
        let chunk_type = u64::from_le_bytes(leaf[off + 0x18..off + 0x20].try_into().unwrap());
        if chunk_type & SYSTEM != 0 {
            continue;
        }
        damage(&mut leaf[off..]);
        damaged = true;
        break;
    }
    assert!(
        damaged,
        "fixture: the chunk tree holds a non-system chunk item"
    );
    stamp_checksum(leaf, &sb);
    std::fs::write(&path, &bytes).unwrap();
    path
}

fn mount(path: &std::path::Path) -> Result<Filesystem, Error> {
    Filesystem::mount(Arc::new(FileDevice::open(path).unwrap()) as Arc<dyn BlockRead>)
}

#[test]
fn a_chunk_item_that_does_not_parse_fails_the_mount_by_name() {
    let path = image_with_damaged_chunk("zero-stripes", |c| c[0x2c..0x2e].fill(0));
    match mount(&path) {
        Err(Error::BadChunkItem(why)) => assert!(
            why.contains("zero stripes") && why.contains("chunk item at logical 0x"),
            "the refusal should name the reason and the chunk: {why}"
        ),
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a chunk item with zero stripes was dropped and the volume mounted"),
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_chunk_item_with_unaligned_geometry_fails_the_mount() {
    // Stripe 0's physical offset, one byte off a sector boundary.
    let path = image_with_damaged_chunk("unaligned", |c| c[0x38] ^= 0x01);
    match mount(&path) {
        Err(Error::BadChunkItem(why)) => assert!(
            why.contains("aligned") && why.contains("chunk item at logical 0x"),
            "the refusal should name the reason and the chunk: {why}"
        ),
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a chunk item with an unaligned stripe was never geometry-checked"),
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// The control: an undamaged image, whose chunk tree also holds a
/// `DEV_ITEM`, still mounts.
#[test]
fn an_undamaged_image_with_dev_items_in_its_chunk_tree_mounts() {
    let path = image_with_damaged_chunk("control", |_| {});
    mount(&path).unwrap_or_else(|e| panic!("an undamaged image must mount: {e:?}"));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
